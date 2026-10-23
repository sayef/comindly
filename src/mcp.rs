//! Comindly MCP server — the code graph as tools a coding agent calls live.
//!
//! Loads the resolved org graph from LanceDB once at startup (local mmap or S3), holds it in
//! memory, and serves deterministic navigation over stdio:
//!
//!   * `search`       — natural-language / hybrid code search (semantic + lexical + centrality)
//!   * `repos`        — indexed repositories + symbol counts
//!   * `suggest`      — pre-generated questions you can ask (query catalog)
//!   * `find`         — locate symbols by name/path substring
//!   * `zoom`         — 360° view of a symbol (callers, callees, importers, members)
//!   * `ripple`       — org-wide blast radius (who breaks if I change this)
//!   * `thread`       — forward call trace from an entry point
//!   * `flow`         — pre-generated walkthrough of a flow + its live call trace
//!   * `context_pack` — the minimal token-budgeted read-set to change a symbol safely
//!   * `guide`        — the repo's inferred style guide
//!
//! Results are handed back as **markdown by default** (readable for the agent); the same data is
//! also attached as structured JSON. Pass `serve --format json` to make the text block raw JSON.
//! Every result carries an exact `file:line`.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::embed::Embedder;
use crate::graph::{CodeGraph, Node as GNode};
use crate::model::Symbol;
use anyhow::Result;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{schemars, tool, tool_router, ServiceExt};
use serde::{Deserialize, Serialize};

/// Ledger records and when they were read.
type Cached = (std::time::Instant, Vec<crate::ledger::Record>);

/// Per-symbol LLM enrichment keyed by rendered symbol id: `(summary, generated_queries)`.
type Enrichment = HashMap<String, (String, Vec<String>)>;

#[derive(Clone)]
pub struct ComindlyServer {
    graph: Arc<CodeGraph>,
    enrichment: Arc<Enrichment>,
    style_guide: Arc<Option<String>>,
    /// Pre-generated flow walkthroughs by entry-point id: `(narration, flow_questions)`.
    flows: Arc<HashMap<String, (String, Vec<String>)>>,
    /// Symbols by rendered id — used by `search` to look up candidate metadata.
    symbols: Arc<HashMap<String, Symbol>>,
    /// Query embedder for `search`; `None` if the model could not be loaded (offline).
    embedder: Arc<Option<Embedder>>,
    uri: Arc<String>,
    /// The index's tables, held open so LanceDB caches their metadata and indexes.
    tables: Arc<crate::index::Tables>,
    /// Ledger records with when they were read; re-read at most once a minute.
    records: Arc<tokio::sync::Mutex<Option<Cached>>>,
    /// Manifest/lockfile rows for internal-dependency answers.
    packages: Arc<Vec<crate::manifests::PackageRow>>,
    /// Measured conventions (per repo + org baseline), computed at startup from stored facts.
    conventions: Arc<Vec<crate::conventions::Convention>>,
    /// All edges (contracts and blast radius for `preflight`).
    edges: Arc<Vec<crate::model::Edge>>,
    /// Hand results back as markdown text (default). `false` → raw JSON text.
    markdown: bool,
    /// The key records agents write are signed with; this machine's in local mode.
    signer: Option<std::path::PathBuf>,
}

impl ComindlyServer {
    /// The index root holding `_graph` and `_ledger`.
    fn ledger_root(&self) -> String {
        self.uri
            .trim_end_matches('/')
            .strip_suffix("/_graph")
            .unwrap_or(&self.uri)
            .to_string()
    }

    async fn records(&self) -> Vec<crate::ledger::Record> {
        let mut cached = self.records.lock().await;
        if let Some((at, r)) = cached.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(60) {
                return r.clone();
            }
        }
        let r = match crate::ledger::Ledger::open(&self.ledger_root()) {
            Ok(l) => l.all().await.unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        *cached = Some((std::time::Instant::now(), r.clone()));
        r
    }

    /// Show a record this server just wrote without waiting for the next re-read.
    async fn wrote(&self, rec: &crate::ledger::Record) {
        if let Some((_, r)) = self.records.lock().await.as_mut() {
            r.push(rec.clone());
        }
    }

    /// The calling agent: `COMINDLY_AGENT`, else the client name from the MCP handshake; over
    /// HTTP on a server, working for the member whose token it uses.
    fn agent(ctx: &rmcp::service::RequestContext<rmcp::RoleServer>) -> crate::ledger::Provenance {
        let name = crate::config::env("COMINDLY_AGENT")
            .or_else(|| Some(ctx.peer.peer_info()?.client_info.name.clone()));
        let who = ctx
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<crate::server::Who>());
        let identity = match who {
            Some(w) => crate::ledger::Identity {
                actor: format!("actor-{}", &blake3::hash(w.email.as_bytes()).to_hex()[..10]),
                name: Some(w.email.clone()),
            },
            None => crate::config::local_identity(),
        };
        crate::ledger::Provenance::agent("mcp", name, &identity)
    }

    /// Trusted (T2+), still-relevant ledger records about these symbols, as compact lines.
    async fn known(&self, ids: &[String], limit: usize) -> Vec<String> {
        let records = self.records().await;
        if records.is_empty() {
            return Vec::new();
        }
        let symbols: Vec<Symbol> = self.symbols.values().cloned().collect();
        let entries = crate::ledger::derive(&records, &symbols, &crate::config::local_verifier());
        crate::ledger::about(&entries, ids)
            .into_iter()
            .filter(|e| e.trust >= crate::ledger::Trust::T2)
            .filter(|e| {
                matches!(
                    e.status,
                    crate::ledger::Status::Active
                        | crate::ledger::Status::Moved
                        | crate::ledger::Status::Stale
                )
            })
            .take(limit)
            .map(crate::ledger::line)
            .collect()
    }

    async fn write_record(
        &self,
        p: RememberParam,
        by: crate::ledger::Provenance,
    ) -> anyhow::Result<String> {
        let kind = crate::ledger::Kind::parse(&p.kind)
            .ok_or_else(|| anyhow::anyhow!("unknown kind `{}`", p.kind))?;
        let mut anchors = Vec::new();
        for focus in &p.on {
            let idx = self
                .graph
                .lookup(focus)
                .ok_or_else(|| anyhow::anyhow!("no symbol matches `{focus}` — use `find` first"))?;
            let id = self.graph.node(idx).id;
            let s = self
                .symbols
                .get(&id)
                .ok_or_else(|| anyhow::anyhow!("symbol `{id}` not loaded"))?;
            anchors.push(crate::ledger::Anchor::of(s));
        }
        let mut rec = crate::ledger::Record::new(kind, &p.title, anchors, by);
        rec.body = p.body.unwrap_or_default();
        rec.trigger = p.trigger;
        rec.evidence = p.evidence;
        rec.fields = p
            .fields
            .into_iter()
            .map(|(k, v)| (k, serde_json::Value::String(v)))
            .collect();
        crate::ledger::Ledger::open(&self.ledger_root())?
            .signed_by(self.signer.clone())
            .put(&rec)
            .await?;
        self.wrote(&rec).await;
        Ok(format!(
            "recorded {} `{}` (id `{}`); it is unconfirmed until a human reviews it",
            kind.as_str(),
            rec.title,
            rec.id
        ))
    }

    async fn write_review(
        &self,
        p: ReviewParam,
        by: crate::ledger::Provenance,
    ) -> anyhow::Result<String> {
        let ledger =
            crate::ledger::Ledger::open(&self.ledger_root())?.signed_by(self.signer.clone());
        let records = self.records().await;
        anyhow::ensure!(
            p.id.len() >= 6,
            "give at least the first 6 characters of the record id"
        );
        let matches: Vec<_> = records
            .iter()
            .filter(|r| r.id == p.id || r.id.starts_with(&p.id))
            .collect();
        let target = match matches.as_slice() {
            [one] => *one,
            [] => anyhow::bail!("no ledger record `{}`", p.id),
            _ => anyhow::bail!(
                "`{}` matches {} records; give more of the id",
                p.id,
                matches.len()
            ),
        };
        let reason = p.reason.as_deref().unwrap_or_default();
        let rec = crate::ledger::Record::review(target, &p.action, reason, by)?;
        ledger.put(&rec).await?;
        self.wrote(&rec).await;
        Ok(format!("{}: `{}`", p.action, target.title))
    }

    /// The repo a focus names: a repo itself, a file path, or a symbol.
    fn repo_of(&self, focus: &str) -> Option<String> {
        if self.graph.repos().iter().any(|(r, _)| r == focus) {
            return Some(focus.to_string());
        }
        if let Some(s) = self
            .symbols
            .values()
            .find(|s| s.kind == crate::model::SymbolKind::File && s.file_path.ends_with(focus))
        {
            return Some(s.repo.0.clone());
        }
        self.graph.lookup(focus).map(|i| self.graph.node(i).repo)
    }

    /// Build a result node, attaching its LLM summary when available.
    fn node_dto(&self, n: GNode) -> NodeDto {
        let enr = self.enrichment.get(&n.id);
        NodeDto {
            id: n.id,
            name: n.name,
            kind: n.kind,
            repo: n.repo,
            location: n.location,
            signature: n.signature,
            summary: enr.map(|(s, _)| s.clone()),
            queries: enr.map(|(_, q)| q.clone()).unwrap_or_default(),
        }
    }

    /// Wrap a tool DTO into a result: markdown text (default) or JSON text, plus the structured
    /// JSON in every case so structured-content clients keep working.
    fn reply<T: Serialize>(&self, dto: &T, md: String) -> CallToolResult {
        let value = serde_json::to_value(dto).ok();
        let text = if self.markdown {
            md
        } else {
            value
                .as_ref()
                .and_then(|v| serde_json::to_string_pretty(v).ok())
                .unwrap_or_default()
        };
        let mut r = CallToolResult::success(vec![ContentBlock::text(text)]);
        r.structured_content = value;
        r
    }
}

// ---- DTOs (serialized to the agent) -----------------------------------------------------

#[derive(Serialize, schemars::JsonSchema)]
struct NodeDto {
    id: String,
    name: String,
    kind: String,
    repo: String,
    /// `path:line` — jump straight here.
    location: String,
    signature: Option<String>,
    /// One-line LLM summary, when the index was enriched.
    summary: Option<String>,
    /// Natural-language questions this symbol answers (pre-generated during `--enrich`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    queries: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct HopDto {
    name: String,
    kind: String,
    repo: String,
    location: String,
    depth: usize,
    via: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RepoCount {
    repo: String,
    symbols: usize,
    /// Packages published by other indexed repos that this repo depends on.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<DepDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct DepDto {
    package: String,
    publisher: String,
    published: Option<String>,
    locked: Option<String>,
    /// Pinned to a different version than the publisher's source.
    skew: bool,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ReposDto {
    repos: Vec<RepoCount>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct FindDto {
    results: Vec<NodeDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ThreadDto {
    focus: String,
    trace: Vec<HopDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct FlowDto {
    focus: String,
    found: bool,
    /// Pre-generated LLM walkthrough (present only if the index was built with `--flows`).
    narration: Option<String>,
    queries: Vec<String>,
    trace: Vec<HopDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ZoomDto {
    found: bool,
    focus: Option<NodeDto>,
    container: Option<NodeDto>,
    callers: Vec<NodeDto>,
    callees: Vec<NodeDto>,
    importers: Vec<NodeDto>,
    members: Vec<NodeDto>,
    /// Service contracts (HTTP routes, queues, buckets, tables) this symbol takes part in.
    contracts: Vec<ContractRoleDto>,
    /// When the focus is a contract: its providers and consumers.
    providers: Vec<NodeDto>,
    consumers: Vec<NodeDto>,
    /// Recorded constraints, failed attempts and decisions about this symbol or its containers.
    known: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ContractRoleDto {
    role: String,
    contract: String,
    /// Participants on the other side (consumers of what it serves, providers of what it uses).
    counterparts: Vec<NodeDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ContractDto {
    contract: String,
    providers: Vec<NodeDto>,
    consumers: Vec<NodeDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ContractsDto {
    contracts: Vec<ContractDto>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct ContractsParam {
    /// Only this contract type: `http`, `sqs`, `sns`, `s3` or `table`.
    #[serde(default)]
    kind: Option<String>,
    /// Only contracts missing a provider or consumer (calls to nothing, unused routes).
    #[serde(default)]
    dangling: Option<bool>,
    /// Substring filter on the contract key, e.g. `/courses` or `orders`.
    #[serde(default)]
    query: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct RippleDto {
    focus: String,
    found: bool,
    total_dependents: usize,
    by_repo: Vec<RepoCount>,
    dependents: Vec<HopDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PackItem {
    name: String,
    location: String,
    est_tokens: usize,
    summary: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct GuideDto {
    found: bool,
    /// Repo the conventions are scoped to (from `focus`), if any.
    repo: Option<String>,
    /// Measured conventions for the repo, then the org baseline.
    conventions: Vec<ConventionDto>,
    /// LLM-written narrative guide (present only if the index was built with `--guide`).
    guide: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct ConventionDto {
    scope: String,
    dimension: String,
    rule: String,
    exemplars: Vec<String>,
    counterexamples: Vec<String>,
    /// Written or edited by a person rather than measured.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    team: bool,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct RememberParam {
    /// `decision` | `attempt` | `constraint` | `gotcha` | `intent` | `question`.
    kind: String,
    /// Symbols (ids or names) the record is about.
    on: Vec<String>,
    /// One neutral, factual line.
    title: String,
    #[serde(default)]
    body: Option<String>,
    /// When a future engineer would need this.
    #[serde(default)]
    trigger: Option<String>,
    /// Evidence: `commit:<sha>`, `mr:!123`, `test:<path>::<name>`, error text… Required for `attempt`.
    #[serde(default)]
    evidence: Vec<String>,
    /// Kind-specific fields, e.g. `{"outcome": "reverted"}`, `{"alternatives": "…"}`.
    #[serde(default)]
    fields: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct PreflightParam {
    /// Files you plan to change (repo-relative or absolute paths).
    #[serde(default)]
    files: Vec<String>,
    /// Symbols you plan to change (ids or names).
    #[serde(default)]
    symbols: Vec<String>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct ReviewParam {
    /// Record id (or a unique prefix) from `why`.
    id: String,
    /// `confirm` | `archive` (no longer applies) | `invalidate` (was wrong) | `supersede` | `dispute`.
    action: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct LedgerDto {
    ok: bool,
    message: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct GuideParam {
    /// A repo name, file path or symbol: scopes conventions to its repo. Omit for org-wide.
    #[serde(default)]
    focus: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct PackDto {
    focus: String,
    found: bool,
    est_total_tokens: usize,
    token_budget: usize,
    items: Vec<PackItem>,
    /// Recorded constraints and failed attempts about the focus and the packed symbols.
    known: Vec<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SearchHitDto {
    /// Stable handle to pass to zoom/ripple/thread/flow/context_pack.
    id: String,
    name: String,
    kind: String,
    repo: String,
    location: String,
    signature: Option<String>,
    score: f32,
    deps: usize,
    summary: Option<String>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SearchDto {
    query: String,
    results: Vec<SearchHitDto>,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SuggestItem {
    query: String,
    symbol: String,
    location: String,
}

#[derive(Serialize, schemars::JsonSchema)]
struct SuggestDto {
    total: usize,
    suggestions: Vec<SuggestItem>,
}

// ---- tool parameters --------------------------------------------------------------------

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct NoArgs {}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct SearchParam {
    /// A natural-language question ("how do we create a migration?") or a symbol name.
    query: String,
    /// Max results (default 12).
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct SuggestParam {
    /// Optional keyword to filter suggestions (matches the question or the symbol name).
    about: Option<String>,
    /// Max suggestions (default 40).
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct FindParam {
    /// Symbol name or descriptor/path substring.
    query: String,
    /// Max results (default 20).
    limit: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct FocusParam {
    /// A symbol name or SCIP descriptor (e.g. `acme/const/Settings`).
    focus: String,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct DepthParam {
    /// A symbol name or SCIP descriptor (from `find`/`search`, e.g. `acme/const/Settings`).
    focus: String,
    /// Max hops to traverse (default 4).
    depth: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema, Default)]
struct PackParam {
    /// A symbol name or SCIP descriptor (from `find`/`search`, e.g. `acme/const/Settings`).
    focus: String,
    /// Token budget for the read-set (default 1500).
    token_budget: Option<u32>,
}

// ---- tools ------------------------------------------------------------------------------

/// First-contact orientation shown to the agent as MCP server `instructions`.
const INSTRUCTIONS: &str = "Deterministic code intelligence over an indexed codebase.\n\
New here? Call `repos` to see what's indexed and `suggest` for ready-made questions.\n\
Workflow: `find`/`search` return a symbol `id` → pass that id to `zoom` (360° view), \
`ripple` (who breaks if I change this — crosses service contracts too), `thread` (forward call \
trace), `context_pack` (minimal safe edit read-set), or `flow` (narrated walkthrough). \
`contracts` lists HTTP routes, queues, topics, buckets and tables with providers and consumers. `search` needs embeddings \
(on by default unless the index was built with --no-embed); the graph tools always work.";

#[tool_router(vis = "pub(crate)")]
impl ComindlyServer {
    #[tool(
        name = "search",
        description = "Natural-language + lexical hybrid code search, reranked by dependency-graph centrality. Ask a question ('how do we create a migration?') or pass a symbol name."
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParam>) -> CallToolResult {
        let limit = p.limit.unwrap_or(12) as usize;
        let Some(embedder) = self.embedder.as_ref() else {
            let dto = SearchDto {
                query: p.query.clone(),
                results: vec![],
            };
            return self.reply(
                &dto,
                "_search unavailable: the embedding model could not be loaded (offline?). Graph tools (find/zoom/ripple/thread/context_pack) still work._".into(),
            );
        };
        let hits = crate::search::hybrid(
            &self.tables,
            &self.symbols,
            &self.graph,
            &self.enrichment,
            embedder,
            &p.query,
            limit,
        )
        .await
        .unwrap_or_default();
        let md = crate::search::markdown(&p.query, &hits);
        let dto = SearchDto {
            query: p.query.clone(),
            results: hits.iter().map(hit_dto).collect(),
        };
        self.reply(&dto, md)
    }

    #[tool(
        name = "repos",
        description = "List indexed repositories and their symbol counts"
    )]
    fn repos(&self, _p: Parameters<NoArgs>) -> CallToolResult {
        let dto = ReposDto {
            repos: self
                .graph
                .repos()
                .into_iter()
                .map(|(repo, symbols)| RepoCount {
                    depends_on: comindly_deps(&self.packages, &repo),
                    repo,
                    symbols,
                })
                .collect(),
        };
        let md = md_repos(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "suggest",
        description = "Suggested questions you can ask about this codebase (pre-generated during --enrich). Optionally filter by a keyword to discover what flows/topics exist."
    )]
    fn suggest(&self, Parameters(p): Parameters<SuggestParam>) -> CallToolResult {
        let limit = p.limit.unwrap_or(40) as usize;
        let about = p.about.as_deref().map(str::to_lowercase);
        let mut items: Vec<SuggestItem> = Vec::new();
        for (id, (_, queries)) in self.enrichment.iter() {
            let (symbol, location) = self
                .symbols
                .get(id)
                .map(|s| {
                    (
                        s.name.clone(),
                        format!("{}:{}", s.file_path, s.range.start.line),
                    )
                })
                .unwrap_or_else(|| (id.clone(), String::new()));
            for q in queries {
                if let Some(a) = &about {
                    if !q.to_lowercase().contains(a) && !symbol.to_lowercase().contains(a) {
                        continue;
                    }
                }
                items.push(SuggestItem {
                    query: q.clone(),
                    symbol: symbol.clone(),
                    location: location.clone(),
                });
            }
        }
        items.sort_by(|a, b| a.symbol.cmp(&b.symbol).then(a.query.cmp(&b.query)));
        let total = items.len();
        items.truncate(limit);
        let dto = SuggestDto {
            total,
            suggestions: items,
        };
        let md = md_suggest(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "find",
        description = "Find symbols by name or path substring. Returns each match with its `id` and `file:line` — the entry point to the graph tools: pass an `id` to zoom/ripple/thread/context_pack/flow."
    )]
    fn find(&self, Parameters(p): Parameters<FindParam>) -> CallToolResult {
        let limit = p.limit.unwrap_or(20) as usize;
        let dto = FindDto {
            results: self
                .graph
                .find(&p.query, limit)
                .into_iter()
                .map(|n| self.node_dto(n))
                .collect(),
        };
        let md = md_find(&p.query, &dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "zoom",
        description = "360° view of one symbol — definition, container, callers, callees, importers, members. Use to understand a symbol before changing it. Takes a symbol id/name from find/search."
    )]
    async fn zoom(&self, Parameters(p): Parameters<FocusParam>) -> CallToolResult {
        let dto = match self.graph.lookup(&p.focus) {
            None => ZoomDto {
                found: false,
                focus: None,
                container: None,
                callers: vec![],
                callees: vec![],
                importers: vec![],
                members: vec![],
                contracts: vec![],
                providers: vec![],
                consumers: vec![],
                known: vec![],
            },
            Some(idx) => {
                let z = self.graph.zoom(idx);
                ZoomDto {
                    found: true,
                    focus: z.focus.map(|n| self.node_dto(n)),
                    container: z.container.map(|n| self.node_dto(n)),
                    callers: z.callers.into_iter().map(|n| self.node_dto(n)).collect(),
                    callees: z.callees.into_iter().map(|n| self.node_dto(n)).collect(),
                    importers: z.importers.into_iter().map(|n| self.node_dto(n)).collect(),
                    members: z.members.into_iter().map(|n| self.node_dto(n)).collect(),
                    contracts: z
                        .contracts
                        .into_iter()
                        .map(|c| ContractRoleDto {
                            role: c.role.to_string(),
                            contract: c.contract.name,
                            counterparts: c
                                .counterparts
                                .into_iter()
                                .map(|n| self.node_dto(n))
                                .collect(),
                        })
                        .collect(),
                    providers: z.providers.into_iter().map(|n| self.node_dto(n)).collect(),
                    consumers: z.consumers.into_iter().map(|n| self.node_dto(n)).collect(),
                    known: match self.graph.scope_of(&p.focus) {
                        Some((_, ids)) => self.known(&ids, 5).await,
                        None => vec![],
                    },
                }
            }
        };
        let md = md_zoom(&p.focus, &dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "contracts",
        description = "Service contracts across repos: HTTP routes ↔ clients, SQS/SNS queues and topics, S3 buckets, DB tables — each with providers and consumers. Use to see how services talk, find calls to routes nobody serves (`dangling`), or filter by `kind`/`query`. `ripple` already crosses these."
    )]
    fn contracts(&self, Parameters(p): Parameters<ContractsParam>) -> CallToolResult {
        let views = self
            .graph
            .contracts(p.kind.as_deref(), p.dangling.unwrap_or(false));
        let dto = ContractsDto {
            contracts: views
                .into_iter()
                .filter(|v| {
                    p.query
                        .as_deref()
                        .is_none_or(|q| v.contract.name.contains(q))
                })
                .take(200)
                .map(|v| ContractDto {
                    contract: v.contract.name,
                    providers: v.providers.into_iter().map(|n| self.node_dto(n)).collect(),
                    consumers: v.consumers.into_iter().map(|n| self.node_dto(n)).collect(),
                })
                .collect(),
        };
        let md = md_contracts(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "why",
        description = "Reasoning blame for a symbol: recorded decisions, failed attempts, constraints, gotchas and intents about it or its class/file — with status (active / stale since the code changed / orphaned), trust and evidence. Check before changing code so you don't repeat a failed approach or break a constraint."
    )]
    async fn why(&self, Parameters(p): Parameters<FocusParam>) -> CallToolResult {
        let Some((name, ids)) = self.graph.scope_of(&p.focus) else {
            let dto = LedgerDto {
                ok: false,
                message: format!("no symbol matches `{}`", p.focus),
            };
            let md = format!("_{}_", dto.message);
            return self.reply(&dto, md);
        };
        let records = self.records().await;
        let symbols: Vec<Symbol> = self.symbols.values().cloned().collect();
        let entries = crate::ledger::derive(&records, &symbols, &crate::config::local_verifier());
        let about = crate::ledger::about(&entries, &ids);
        let md = crate::ledger::markdown(&format!("why `{name}`"), &about);
        let dto = LedgerDto {
            ok: true,
            message: md.clone(),
        };
        self.reply(&dto, md)
    }

    #[tool(
        name = "preflight",
        description = "Before you change code: given the files and/or symbols you plan to edit, returns recorded constraints and failed attempts about them, contracts (routes, tables, queues) other services consume, blast radius across repos, and directories that usually change together but aren't in your plan. Run it on your plan before editing."
    )]
    async fn preflight(&self, Parameters(p): Parameters<PreflightParam>) -> CallToolResult {
        let mut changes = Vec::new();
        for f in &p.files {
            if let Some(s) = self.symbols.values().find(|s| {
                s.kind == crate::model::SymbolKind::File
                    && (s.file_path == *f || f.ends_with(&format!("/{}", s.file_path)))
            }) {
                changes.push(crate::preflight::Change {
                    repo: s.repo.0.clone(),
                    path: s.file_path.clone(),
                    lines: Vec::new(),
                });
            }
        }
        for focus in &p.symbols {
            let Some(idx) = self.graph.lookup(focus) else {
                continue;
            };
            if let Some(s) = self.symbols.get(&self.graph.node(idx).id) {
                changes.push(crate::preflight::Change {
                    repo: s.repo.0.clone(),
                    path: s.file_path.clone(),
                    lines: vec![(s.range.start.line, s.range.end.line)],
                });
            }
        }
        let records = self.records().await;
        let symbols: Vec<Symbol> = self.symbols.values().cloned().collect();
        let entries = crate::ledger::derive(&records, &symbols, &crate::config::local_verifier());
        let findings = crate::checks::run(&entries, &symbols, &self.edges, &self.packages);
        let recipes: Vec<crate::conventions::Convention> = self
            .conventions
            .iter()
            .filter(|c| c.dimension == "recipes")
            .cloned()
            .collect();
        let md = if changes.is_empty() {
            "_no indexed files or symbols matched — pass repo-relative paths or names from `find`._"
                .to_string()
        } else {
            crate::preflight::report(
                &self.graph,
                &symbols,
                &self.edges,
                &entries,
                &recipes,
                &findings,
                &changes,
            )
        };
        let dto = LedgerDto {
            ok: !changes.is_empty(),
            message: md.clone(),
        };
        self.reply(&dto, md)
    }

    #[tool(
        name = "remember",
        description = "Record durable engineering knowledge about symbols for the team: a failed or reverted approach (`attempt`, evidence required), a choice between alternatives (`decision`), an invariant (`constraint`), a trap (`gotcha`), why code exists (`intent`) or an open `question`. Only record what a future engineer would be glad to know before editing — not summaries of what you did. Write neutral facts, never instructions."
    )]
    async fn remember(
        &self,
        ctx: rmcp::service::RequestContext<rmcp::RoleServer>,
        Parameters(p): Parameters<RememberParam>,
    ) -> CallToolResult {
        let result = self.write_record(p, Self::agent(&ctx)).await;
        let dto = LedgerDto {
            ok: result.is_ok(),
            message: result.unwrap_or_else(|e| format!("not recorded: {e:#}")),
        };
        let md = dto.message.clone();
        self.reply(&dto, md)
    }

    #[tool(
        name = "review",
        description = "Act on a ledger record after checking it against the code: `confirm` (still true), `archive` (was true, no longer applies), `invalidate` (was wrong), `supersede` (replaced by a newer record) or `dispute` (contradicted). Give a reason. Retiring a record a person wrote or confirmed only flags it for a person to decide. Use when `why` shows a STALE record you have verified."
    )]
    async fn review(
        &self,
        ctx: rmcp::service::RequestContext<rmcp::RoleServer>,
        Parameters(p): Parameters<ReviewParam>,
    ) -> CallToolResult {
        let result = self.write_review(p, Self::agent(&ctx)).await;
        let dto = LedgerDto {
            ok: result.is_ok(),
            message: result.unwrap_or_else(|e| format!("not recorded: {e:#}")),
        };
        let md = dto.message.clone();
        self.reply(&dto, md)
    }

    #[tool(
        name = "ripple",
        description = "Blast radius (reverse deps): who transitively depends on this symbol, cross-repo, grouped by repo. Use to gauge the impact of changing it. This is the reverse direction of `thread`. Returns dependents with depth + repo counts."
    )]
    fn ripple(&self, Parameters(p): Parameters<DepthParam>) -> CallToolResult {
        let depth = p.depth.unwrap_or(4) as usize;
        let dto = match self.graph.lookup(&p.focus) {
            None => RippleDto {
                focus: p.focus.clone(),
                found: false,
                total_dependents: 0,
                by_repo: vec![],
                dependents: vec![],
            },
            Some(idx) => {
                let hops = self.graph.ripple(idx, depth);
                let mut by_repo: std::collections::BTreeMap<String, usize> =
                    std::collections::BTreeMap::new();
                for h in &hops {
                    *by_repo.entry(h.node.repo.clone()).or_default() += 1;
                }
                RippleDto {
                    focus: p.focus.clone(),
                    found: true,
                    total_dependents: hops.len(),
                    by_repo: by_repo
                        .into_iter()
                        .map(|(repo, symbols)| RepoCount {
                            repo,
                            symbols,
                            depends_on: Vec::new(),
                        })
                        .collect(),
                    dependents: hops.into_iter().map(hop_dto).collect(),
                }
            }
        };
        let md = md_ripple(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "guide",
        description = "Measured coding conventions to follow when editing: naming, signatures, base classes, error roots, layout, logging, config, internal libraries to use instead of raw ones, testing — each with counts, exemplar locations and known deviations. Pass `focus` (repo, file or symbol) to scope to its repo; the org baseline is always included."
    )]
    fn guide(&self, Parameters(p): Parameters<GuideParam>) -> CallToolResult {
        let repo = p.focus.as_deref().and_then(|f| self.repo_of(f));
        let repo_keys: std::collections::HashSet<&str> = self
            .conventions
            .iter()
            .filter(|c| Some(&c.repo) == repo.as_ref())
            .map(|c| c.key.as_str())
            .collect();
        let mut conventions: Vec<ConventionDto> = Vec::new();
        for c in self.conventions.iter() {
            let scoped = Some(&c.repo) == repo.as_ref();
            let org = c.repo == crate::conventions::ORG && !repo_keys.contains(c.key.as_str());
            if scoped || org {
                conventions.push(ConventionDto {
                    scope: c.repo.clone(),
                    dimension: c.dimension.clone(),
                    rule: c.rule.clone(),
                    exemplars: c.exemplars.iter().take(2).cloned().collect(),
                    counterexamples: c.counterexamples.iter().take(2).cloned().collect(),
                    team: c.origin != crate::conventions::Origin::Measured,
                });
            }
        }
        let dto = GuideDto {
            found: !conventions.is_empty() || self.style_guide.is_some(),
            repo,
            conventions,
            guide: (*self.style_guide).clone(),
        };
        let md = md_guide(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "thread",
        description = "Forward call trace from an entry point: what this symbol calls, transitively, with depth + edge kind. Use to follow execution downstream. `flow` = this trace plus a narrated explanation; `ripple` = the reverse (who calls in)."
    )]
    fn thread(&self, Parameters(p): Parameters<DepthParam>) -> CallToolResult {
        let depth = p.depth.unwrap_or(4) as usize;
        let trace = match self.graph.lookup(&p.focus) {
            Some(idx) => self
                .graph
                .thread(idx, depth)
                .into_iter()
                .map(hop_dto)
                .collect(),
            None => vec![],
        };
        let dto = ThreadDto {
            focus: p.focus.clone(),
            trace,
        };
        let md = md_thread(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "flow",
        description = "Explain how a flow works: a pre-generated LLM walkthrough of an entry point (when the index was built with --flows) plus its live forward call trace."
    )]
    fn flow(&self, Parameters(p): Parameters<DepthParam>) -> CallToolResult {
        let depth = p.depth.unwrap_or(4) as usize;
        let dto = match self.graph.lookup(&p.focus) {
            None => FlowDto {
                focus: p.focus.clone(),
                found: false,
                narration: None,
                queries: vec![],
                trace: vec![],
            },
            Some(idx) => {
                let fid = self.graph.zoom(idx).focus.map(|n| n.id);
                let (narration, queries) = fid
                    .as_ref()
                    .and_then(|id| self.flows.get(id))
                    .map(|(n, q)| (Some(n.clone()), q.clone()))
                    .unwrap_or((None, vec![]));
                let trace = self
                    .graph
                    .thread(idx, depth)
                    .into_iter()
                    .map(hop_dto)
                    .collect();
                FlowDto {
                    focus: p.focus.clone(),
                    found: true,
                    narration,
                    queries,
                    trace,
                }
            }
        };
        let md = md_flow(&dto);
        self.reply(&dto, md)
    }

    #[tool(
        name = "context_pack",
        description = "The minimal token-budgeted set of symbols to read before editing the focus symbol safely. Use to assemble just-enough context for a change. Returns ranked symbols with locations + token estimates within the budget."
    )]
    async fn context_pack(&self, Parameters(p): Parameters<PackParam>) -> CallToolResult {
        let budget = p.token_budget.unwrap_or(1500) as usize;
        let dto = match self.graph.lookup(&p.focus) {
            None => PackDto {
                focus: p.focus.clone(),
                found: false,
                est_total_tokens: 0,
                token_budget: budget,
                items: vec![],
                known: vec![],
            },
            Some(idx) => {
                let pack = self.graph.context_pack(idx, budget);
                let est_total_tokens = pack.iter().map(|(_, t)| t).sum();
                let mut ids: Vec<String> = pack.iter().map(|(n, _)| n.id.clone()).collect();
                if let Some((_, scope)) = self.graph.scope_of(&p.focus) {
                    ids.extend(scope);
                }
                PackDto {
                    focus: p.focus.clone(),
                    found: true,
                    est_total_tokens,
                    token_budget: budget,
                    known: self.known(&ids, 6).await,
                    items: pack
                        .into_iter()
                        .map(|(n, t)| PackItem {
                            summary: self.enrichment.get(&n.id).map(|(s, _)| s.clone()),
                            name: n.name,
                            location: n.location,
                            est_tokens: t,
                        })
                        .collect(),
                }
            }
        };
        let md = md_pack(&dto);
        self.reply(&dto, md)
    }
}

#[rmcp::tool_handler(router = Self::tool_router())]
impl rmcp::ServerHandler for ComindlyServer {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(rmcp::model::Implementation::from_build_env())
        .with_instructions(INSTRUCTIONS)
    }
}

fn hop_dto(h: crate::graph::Hop) -> HopDto {
    HopDto {
        name: h.node.name,
        kind: h.node.kind,
        repo: h.node.repo,
        location: h.node.location,
        depth: h.depth,
        via: h.via.as_str().to_string(),
    }
}

fn hit_dto(h: &crate::search::SearchHit) -> SearchHitDto {
    SearchHitDto {
        id: h.id.clone(),
        name: h.name.clone(),
        kind: h.kind.clone(),
        repo: h.repo.clone(),
        location: h.location.clone(),
        signature: h.signature.clone(),
        score: h.score,
        deps: h.deps,
        summary: h.summary.clone(),
    }
}

// ---- markdown rendering (the agent-facing handover) -------------------------------------

fn node_bullet(o: &mut String, n: &NodeDto) {
    let _ = write!(o, "- **{}** _{}_ — `{}`", n.name, n.kind, n.location);
    if !n.repo.is_empty() {
        let _ = write!(o, " · {}", n.repo);
    }
    if let Some(s) = &n.summary {
        let _ = write!(o, " — {s}");
    }
    // Stable handle to pass back to zoom/ripple/thread/flow/context_pack.
    let _ = write!(o, "  ·  id `{}`", n.id);
    o.push('\n');
}

fn node_section(o: &mut String, title: &str, nodes: &[NodeDto]) {
    if nodes.is_empty() {
        return;
    }
    let _ = write!(o, "\n**{title}** ({})\n", nodes.len());
    for n in nodes {
        node_bullet(o, n);
    }
}

fn md_repos(d: &ReposDto) -> String {
    let mut o = String::from("## Indexed repositories\n\n");
    if d.repos.is_empty() {
        return o + "_none_\n";
    }
    for r in &d.repos {
        let _ = writeln!(o, "- **{}** — {} symbols", r.repo, r.symbols);
        for dep in &r.depends_on {
            let _ = writeln!(
                o,
                "  - uses `{}` from **{}** — locked {} vs published {}{}",
                dep.package,
                dep.publisher,
                dep.locked.as_deref().unwrap_or("?"),
                dep.published.as_deref().unwrap_or("?"),
                if dep.skew { " ⚠ skew" } else { "" }
            );
        }
    }
    o
}

fn comindly_deps(rows: &[crate::manifests::PackageRow], repo: &str) -> Vec<DepDto> {
    crate::manifests::internal(rows)
        .into_iter()
        .filter(|d| d.consumer == repo)
        .map(|d| DepDto {
            skew: d.skewed(),
            package: d.name,
            publisher: d.publisher,
            published: d.published,
            locked: d.locked,
        })
        .collect()
}

fn md_suggest(d: &SuggestDto) -> String {
    let shown = d.suggestions.len();
    let mut o = if d.total > shown {
        format!(
            "## Suggested questions ({shown} of {} — pass `about` to filter)\n\n",
            d.total
        )
    } else {
        format!("## Suggested questions ({shown})\n\n")
    };
    if d.suggestions.is_empty() {
        return o + "_none — index was built without `--enrich`, or no match_\n";
    }
    let mut cur = "";
    for it in &d.suggestions {
        if it.symbol != cur {
            let _ = write!(o, "\n**{}** `{}`\n", it.symbol, it.location);
            cur = &it.symbol;
        }
        let _ = writeln!(o, "- {}", it.query);
    }
    o
}

fn md_find(query: &str, d: &FindDto) -> String {
    let mut o = format!("## `find` \"{query}\" — {} result(s)\n\n", d.results.len());
    if d.results.is_empty() {
        return o + "_no symbols matched_\n";
    }
    for n in &d.results {
        node_bullet(&mut o, n);
    }
    o
}

fn md_zoom(focus: &str, d: &ZoomDto) -> String {
    if !d.found {
        return format!("## zoom `{focus}`\n\n_no symbol matched — call `find` with the name to get its id, then retry._\n");
    }
    let mut o = String::new();
    if let Some(f) = &d.focus {
        let _ = writeln!(o, "## {} _{}_\n`{}`", f.name, f.kind, f.location);
        if let Some(sig) = &f.signature {
            let _ = writeln!(o, "\n```\n{sig}\n```");
        }
        if let Some(s) = &f.summary {
            let _ = writeln!(o, "\n{s}");
        }
        if !f.queries.is_empty() {
            let _ = writeln!(o, "\n**Ask:** {}", f.queries.join(" · "));
        }
    } else {
        let _ = writeln!(o, "## zoom `{focus}`");
    }
    if let Some(c) = &d.container {
        let _ = write!(o, "\n**Container**\n");
        node_bullet(&mut o, c);
    }
    node_section(&mut o, "Members", &d.members);
    node_section(&mut o, "Callers", &d.callers);
    node_section(&mut o, "Callees", &d.callees);
    node_section(&mut o, "Importers", &d.importers);
    if !d.known.is_empty() {
        o.push_str(
            "\n**Known constraints & dead ends** (team ledger — verify before relying on them)\n",
        );
        for k in &d.known {
            let _ = writeln!(o, "- {k}");
        }
    }
    node_section(&mut o, "Provided by", &d.providers);
    node_section(&mut o, "Used by", &d.consumers);
    if !d.contracts.is_empty() {
        let _ = write!(o, "\n**Contracts**\n");
        for c in &d.contracts {
            let _ = writeln!(o, "- {} `{}`", c.role, c.contract);
            for n in &c.counterparts {
                let _ = writeln!(o, "  - **{}** ({}) `{}`", n.name, n.repo, n.location);
            }
        }
    }
    o
}

fn md_contracts(d: &ContractsDto) -> String {
    let mut o = format!("## {} contract(s)\n", d.contracts.len());
    for c in &d.contracts {
        let _ = writeln!(o, "\n### `{}`", c.contract);
        let side = |o: &mut String, label: &str, ns: &[NodeDto]| {
            if ns.is_empty() {
                let _ = writeln!(o, "- {label}: _none indexed_");
            }
            for n in ns {
                let _ = writeln!(o, "- {label}: **{}** ({}) `{}`", n.name, n.repo, n.location);
            }
        };
        side(&mut o, "provided by", &c.providers);
        side(&mut o, "used by", &c.consumers);
    }
    o
}

fn md_hop_lines(o: &mut String, hops: &[HopDto]) {
    for h in hops {
        let _ = writeln!(
            o,
            "- _d{}_ via **{}** — **{}** `{}`",
            h.depth, h.via, h.name, h.location
        );
    }
}

fn md_ripple(d: &RippleDto) -> String {
    if !d.found {
        return format!("## Blast radius: `{}`\n\n_no symbol matched — call `find` with the name to get its id, then retry._\n", d.focus);
    }
    let mut o = format!(
        "## Blast radius: `{}`\n\n**{} dependent(s)**",
        d.focus, d.total_dependents
    );
    if !d.by_repo.is_empty() {
        let by: Vec<String> = d
            .by_repo
            .iter()
            .map(|r| format!("{} ({})", r.repo, r.symbols))
            .collect();
        let _ = write!(o, " across {}", by.join(", "));
    }
    o.push_str("\n\n");
    md_hop_lines(&mut o, &d.dependents);
    o
}

fn md_thread(d: &ThreadDto) -> String {
    let mut o = format!(
        "## Call trace from `{}` — {} step(s)\n\n",
        d.focus,
        d.trace.len()
    );
    if d.trace.is_empty() {
        return o + "_no outgoing calls found_\n";
    }
    md_hop_lines(&mut o, &d.trace);
    o
}

fn md_flow(d: &FlowDto) -> String {
    if !d.found {
        return format!("## Flow: `{}`\n\n_no symbol matched — call `find` with the name to get its id, then retry._\n", d.focus);
    }
    let mut o = format!("## Flow: `{}`\n\n", d.focus);
    match &d.narration {
        Some(n) => {
            let _ = writeln!(o, "{n}\n");
        }
        None => {
            let _ = writeln!(
                o,
                "_no pre-generated narration (build the index with `--flows` to add one) — raw call trace below._\n"
            );
        }
    }
    if !d.queries.is_empty() {
        let _ = writeln!(o, "**Ask:** {}\n", d.queries.join(" · "));
    }
    let _ = writeln!(o, "**Call trace** ({} steps)", d.trace.len());
    md_hop_lines(&mut o, &d.trace);
    o
}

fn md_pack(d: &PackDto) -> String {
    if !d.found {
        return format!("## Context pack: `{}`\n\n_no symbol matched — call `find` with the name to get its id, then retry._\n", d.focus);
    }
    let mut o = format!(
        "## Context to change `{}`\n\n_~{} of {} token budget · {} symbols_\n\n",
        d.focus,
        d.est_total_tokens,
        d.token_budget,
        d.items.len()
    );
    if !d.known.is_empty() {
        o.push_str(
            "**Known constraints & dead ends** (team ledger — verify before relying on them)\n",
        );
        for k in &d.known {
            let _ = writeln!(o, "- {k}");
        }
        o.push('\n');
    }
    for (i, it) in d.items.iter().enumerate() {
        let _ = write!(
            o,
            "{}. **{}** — `{}` _(~{} tok)_",
            i + 1,
            it.name,
            it.location,
            it.est_tokens
        );
        if let Some(s) = &it.summary {
            let _ = write!(o, " — {s}");
        }
        o.push('\n');
    }
    o
}

fn md_guide(d: &GuideDto) -> String {
    let mut o = match &d.repo {
        Some(r) => format!("## Conventions — {r}\n"),
        None => "## Conventions — org baseline\n".to_string(),
    };
    if d.conventions.is_empty() {
        o.push_str("\n_none measured — re-run `comindly index`/`link` to store facts._\n");
    }
    let mut org_header = false;
    for c in &d.conventions {
        if c.scope == crate::conventions::ORG && d.repo.is_some() && !org_header {
            o.push_str("\n**Org baseline**\n");
            org_header = true;
        }
        let team = if c.team { " _(set by the team)_" } else { "" };
        let _ = writeln!(o, "- _{}_: {}{team}", c.dimension, c.rule);
        if !c.exemplars.is_empty() {
            let _ = writeln!(o, "  - e.g. `{}`", c.exemplars.join("`, `"));
        }
        if !c.counterexamples.is_empty() {
            let _ = writeln!(o, "  - deviations: `{}`", c.counterexamples.join("`, `"));
        }
    }
    if let Some(g) = &d.guide {
        let _ = write!(o, "\n## Narrative guide\n\n{g}\n");
    }
    o
}

/// Load the graph (and any LLM enrichment) from `uri` and serve MCP over stdio until the
/// client disconnects. `markdown` controls whether tool results are handed back as markdown
/// text (default) or raw JSON.
pub async fn serve_stdio(uri: &str, markdown: bool) -> Result<()> {
    let mut server = load(uri, markdown, crate::config::Config::load().fresh()).await?;
    server.signer = crate::config::local_signing_key();
    let service = server.serve(rmcp::transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}

/// Load the graph, enrichment, conventions and search model for `uri` into a server.
pub async fn load(uri: &str, markdown: bool, fresh: std::time::Duration) -> Result<ComindlyServer> {
    let (symbols, edges) = crate::index::read_graph(uri).await?;
    let mut conventions = match crate::index::read_facts(uri).await {
        Ok(Some(facts)) => crate::conventions::mine(&symbols, &edges, &facts),
        _ => Vec::new(),
    };
    conventions.extend(crate::index::read_recipes(uri).await.unwrap_or_default());
    let root = uri.trim_end_matches('/');
    let root = root.strip_suffix("/_graph").unwrap_or(root);
    let manual = match crate::conventions::Store::open(root) {
        Ok(s) => s.all().await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    let conventions = crate::conventions::apply(conventions, &manual);
    let graph = Arc::new(CodeGraph::build(&symbols, &edges));
    let by_id: HashMap<String, Symbol> =
        symbols.iter().map(|s| (s.id.render(), s.clone())).collect();

    let enrichment: Enrichment = crate::index::read_enrichment(uri)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, s, q)| (id.render(), (s, q)))
        .collect();

    // Per-repo style guides, combined into one markdown doc with a section per repo.
    let guides = crate::index::read_style_guide(uri)
        .await
        .unwrap_or_default();
    let style_guide = (!guides.is_empty()).then(|| {
        guides
            .iter()
            .map(|(r, g)| format!("## {r}\n\n{g}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    });

    let flows: HashMap<String, (String, Vec<String>)> = crate::index::read_flows(uri)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, narr, q)| (id.render(), (narr, q)))
        .collect();

    // The embedder powers `search`; graph tools work without it, so a load failure is non-fatal.
    let model = crate::config::index_settings(root).await.embed_model;
    let embedder = match tokio::task::spawn_blocking(move || crate::embed::Embedder::load(&model))
        .await
        .map_err(anyhow::Error::from)
        .and_then(|r| r)
    {
        Ok(e) => Some(e),
        Err(e) => {
            crate::ui::warn(&format!(
                "embedding model unavailable, `search` disabled: {e:#}"
            ));
            None
        }
    };

    let server = ComindlyServer {
        graph,
        enrichment: Arc::new(enrichment),
        style_guide: Arc::new(style_guide),
        flows: Arc::new(flows),
        symbols: Arc::new(by_id),
        embedder: Arc::new(embedder),
        uri: Arc::new(uri.to_string()),
        tables: Arc::new(crate::index::Tables::connect(uri, fresh).await?),
        records: Arc::new(tokio::sync::Mutex::new(None)),
        packages: Arc::new(crate::index::read_packages(uri).await.unwrap_or_default()),
        conventions: Arc::new(conventions),
        edges: Arc::new(edges),
        markdown,
        signer: None,
    };
    Ok(server)
}

/// The server agents reach over HTTP; empty until an index exists, replaced after each re-index.
pub type Shared = Arc<std::sync::RwLock<Option<ComindlyServer>>>;

/// MCP over streamable HTTP for `hosts` (the server's own names), answering from `current`.
pub fn http(
    current: Shared,
    hosts: Vec<String>,
) -> rmcp::transport::streamable_http_server::StreamableHttpService<
    ComindlyServer,
    rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
> {
    let config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default()
        .with_allowed_hosts(hosts);
    rmcp::transport::streamable_http_server::StreamableHttpService::new(
        move || {
            current.read().ok().and_then(|s| s.clone()).ok_or_else(|| {
                std::io::Error::other("no index yet: an admin indexes repositories first")
            })
        },
        Default::default(),
        config,
    )
}

/// The MCP tools this server offers: `(name, description)`.
pub fn tools() -> Vec<(String, String)> {
    ComindlyServer::tool_router()
        .list_all()
        .into_iter()
        .map(|t| {
            (
                t.name.to_string(),
                t.description.unwrap_or_default().to_string(),
            )
        })
        .collect()
}
