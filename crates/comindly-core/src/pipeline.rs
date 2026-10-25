//! Indexing: check out repositories, parse and bind them across repositories, persist the graph,
//! then the optional steps (search embeddings, summaries, style guides, flow walkthroughs).
//! Async and cancellable; CPU-heavy work runs on the blocking pool; progress goes to the context's
//! reporter. LLM output is saved as it is written and reused while its input is unchanged, so an
//! interrupted run continues where it stopped.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use futures::StreamExt;

use crate::context::{Cancelled, IndexContext};
use crate::events::Usage;
use crate::model::{Edge, EdgeKind, GlobalSymbolId, Symbol, SymbolKind};

/// A repository to index: the folder of its clone and, for one comindly keeps itself, the URL it
/// is cloned from when the folder is missing.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct RepoEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
}

/// A repository to index: its name and the folder holding its checkout.
#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub path: PathBuf,
}

impl Source {
    /// A folder, named after itself.
    pub fn at(path: impl Into<PathBuf>) -> Source {
        let path = path.into();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n != "." && n != ".." => n.to_string(),
            _ => std::fs::canonicalize(&path)
                .ok()
                .and_then(|c| c.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "repo".to_string()),
        };
        Source { name, path }
    }
}

/// A symbol many repositories depend on, for the link report.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub descriptor: String,
    pub defined_in: String,
    pub repos: BTreeSet<String>,
    pub refs: usize,
}

/// What a link run produced.
#[derive(Debug, Clone, Default)]
pub struct LinkReport {
    pub symbols: usize,
    pub edges: usize,
    pub cross_repo_edges: usize,
    /// Cross-repository import targets, most depended-on first.
    pub dependencies: Vec<Dependency>,
}

type TextRow = (GlobalSymbolId, String, Vec<String>);

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .context("a worker thread failed")
}

/// Bring each configured repository to its default branch in comindly's own checkout.
pub fn checkout<'a>(
    ctx: &'a IndexContext,
    repos: &'a [RepoEntry],
) -> futures::future::BoxFuture<'a, Result<Vec<Source>>> {
    Box::pin(async move {
        anyhow::ensure!(
            !repos.is_empty(),
            "no repositories: pass folders, or add them with `comindly repos add <folder>`"
        );
        let r = &ctx.report;
        r.step(format!("Checking out {} repo(s)", repos.len()));
        let mut out = Vec::new();
        for repo in repos {
            ctx.check()?;
            let path = PathBuf::from(&repo.path);
            if let Some(url) = repo.remote.clone() {
                if !path.exists() {
                    r.note(format!("cloning {url}"));
                }
                let (dest, git) = (path.clone(), ctx.git.clone());
                blocking(move || {
                    crate::worktree::ensure_clone(&url, &dest, git.as_ref())
                        .map_err(|e| anyhow::anyhow!("{url}: {e:#}"))
                })
                .await??;
            }
            let (dir, git, cache) = (path.clone(), ctx.git.clone(), ctx.cache.clone());
            let c = blocking(move || crate::worktree::prepare(&cache, &dir, git.as_ref()))
                .await?
                .with_context(|| path.display().to_string())?;
            r.done(format!(
                "{} @ {} {}",
                crate::worktree::name(&path),
                c.target,
                c.commit
            ));
            if let Some(why) = &c.stale {
                r.warn(format!(
                    "could not fetch, indexing the last fetched state: {why}"
                ));
            }
            out.push(Source::at(c.dir));
        }
        Ok(out)
    })
}

/// Parse `sources`, bind references across them, persist the graph and run the optional steps.
pub fn link<'a>(
    ctx: &'a IndexContext,
    sources: Vec<Source>,
    incremental: bool,
    scip: crate::scip::Mode,
) -> futures::future::BoxFuture<'a, Result<LinkReport>> {
    Box::pin(async move {
        if let Some(bad) = sources.iter().find(|s| !s.path.is_dir()) {
            anyhow::bail!("no such repo directory: {}", bad.path.display());
        }
        let r = &ctx.report;
        r.step(format!("Parsing {} repo(s)", sources.len()));
        let mut symbols: Vec<Symbol> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut facts = Vec::new();
        for s in &sources {
            ctx.check()?;
            let (name, path) = (s.name.clone(), s.path.clone());
            let (out, scip_note) = blocking(move || -> Result<_> {
                let mut o = crate::parse::parse_repo(&path, &name)
                    .with_context(|| format!("{name}: parse failed"))?;
                let note = attach_scip(scip, &path, &name, &mut o)?;
                Ok((o, note))
            })
            .await??;
            r.note(format!(
                "{}: {} symbols, {} files",
                s.name,
                out.symbols.len(),
                out.facts.len()
            ));
            if let Some(n) = scip_note {
                r.note(n);
            }
            symbols.extend(out.symbols);
            edges.extend(out.edges);
            facts.extend(out.facts);
        }

        ctx.check()?;
        let (symbols, resolved, facts) = blocking(move || {
            let resolved = crate::resolve::resolve(&symbols, &edges, &facts);
            let mut symbols = symbols;
            symbols.extend(resolved.symbols.iter().cloned());
            (symbols, resolved, facts)
        })
        .await?;
        report_resolution(ctx, &resolved);
        let edges = resolved.edges;
        let dependencies = dependencies(&edges);

        let dst = ctx.graph();
        r.step("Persisting");
        r.note(format!("writing org graph to {dst}"));
        let roots: Vec<(String, PathBuf)> = sources
            .iter()
            .map(|s| (s.name.clone(), s.path.clone()))
            .collect();
        let packages = scan_packages(ctx, roots.clone()).await?;
        let recipes = mine_recipes(ctx, roots.clone()).await?;
        let explain = |e: anyhow::Error| {
            anyhow::anyhow!(
                "write failed: {}",
                crate::storage::explain(&format!("{e:#}"))
            )
        };
        let (sv, ev) = crate::index::write_graph(&dst, &symbols, &edges)
            .await
            .map_err(explain)?;
        crate::index::write_facts(&dst, &facts)
            .await
            .map_err(explain)?;
        crate::index::write_packages(&dst, &packages)
            .await
            .map_err(explain)?;
        crate::index::write_recipes(&dst, &recipes)
            .await
            .map_err(explain)?;
        r.done(format!(
            "org graph: symbols v{sv}, edges v{ev} (latest pointer)"
        ));
        let (sc, ec) = crate::index::count_rows(&dst)
            .await
            .context("read-back failed")?;
        r.note(format!("read back: {sc} symbols, {ec} edges"));

        let paths: Vec<(String, PathBuf)> = roots.clone();
        let heads = blocking(move || {
            paths
                .iter()
                .filter_map(|(name, p)| {
                    crate::git::head_commit(p)
                        .ok()
                        .map(|commit| crate::index::IndexedRepo {
                            name: name.clone(),
                            commit,
                            remote: crate::worktree::remote_url(p),
                        })
                })
                .collect::<Vec<_>>()
        })
        .await?;
        let stale: HashSet<String> = if incremental {
            let stale = stale_ids(&dst, roots.clone(), &symbols).await?;
            r.note(format!(
                "incremental: {} of {} symbols stale",
                stale.len(),
                symbols.len()
            ));
            stale
        } else {
            symbols.iter().map(|s| s.id.render()).collect()
        };

        let symbols = Arc::new(symbols);
        let edges = Arc::new(edges);
        enrich_all(ctx, &dst, &symbols, &edges, &stale, incremental, &roots).await?;
        crate::index::write_repo_commits(&dst, &heads).await?;

        Ok(LinkReport {
            symbols: symbols.len(),
            edges: edges.len(),
            cross_repo_edges: edges.iter().filter(|e| e.cross_repo).count(),
            dependencies,
        })
    })
}

/// Index one repository into the shared index: the whole repository, or with `incremental` only
/// the files changed since `since` (or the commit recorded last time).
pub fn index_repo<'a>(
    ctx: &'a IndexContext,
    source: Source,
    incremental: bool,
    since: Option<String>,
    scip: crate::scip::Mode,
) -> futures::future::BoxFuture<'a, Result<()>> {
    Box::pin(async move {
        anyhow::ensure!(
            source.path.is_dir(),
            "no such repo directory: {}",
            source.path.display()
        );
        let r = &ctx.report;
        let dst = ctx.graph();
        let incremental = incremental || since.is_some();
        if incremental {
            let base = match since {
                Some(s) => Some(s),
                None => crate::index::read_repo_meta(&dst).await.ok().flatten(),
            };
            match base {
                Some(base) => return index_changes(ctx, &source, &base).await,
                None => r.note("no recorded base commit — doing a full index"),
            }
        }

        r.step(format!("Indexing {}", source.name));
        let (name, path) = (source.name.clone(), source.path.clone());
        let (symbols, edges, facts) = blocking(move || -> Result<_> {
            let mut out = crate::parse::parse_repo(&path, &name)
                .map_err(|e| anyhow::anyhow!(crate::storage::explain(&format!("{e:#}"))))?;
            attach_scip(scip, &path, &name, &mut out)?;
            let resolved = crate::resolve::resolve(&out.symbols, &out.edges, &out.facts);
            let mut symbols = out.symbols;
            symbols.extend(resolved.symbols);
            Ok((symbols, resolved.edges, out.facts))
        })
        .await??;

        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        for s in &symbols {
            *by_kind.entry(format!("{:?}", s.kind)).or_default() += 1;
        }
        let calls = edges.iter().filter(|e| e.kind == EdgeKind::Calls).count();
        let contains = edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Contains)
            .count();
        r.note(format!("symbols: {}", symbols.len()));
        for (k, n) in &by_kind {
            r.note(format!("{k:<10} {n}"));
        }
        r.note(format!(
            "edges: {} (contains={contains}, calls={calls})",
            edges.len()
        ));

        r.step("Persisting");
        r.note(format!("writing graph to {dst}"));
        let roots = vec![(source.name.clone(), source.path.clone())];
        let explain = |e: anyhow::Error| {
            anyhow::anyhow!(
                "write failed: {}",
                crate::storage::explain(&format!("{e:#}"))
            )
        };
        let (sv, ev) = crate::index::write_graph(&dst, &symbols, &edges)
            .await
            .map_err(explain)?;
        crate::index::write_facts(&dst, &facts)
            .await
            .map_err(explain)?;
        // Keep other repositories' package and recipe rows in a shared index.
        let mut packages: Vec<_> = crate::index::read_packages(&dst)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.repo != source.name)
            .collect();
        packages.extend(scan_packages(ctx, roots.clone()).await?);
        crate::index::write_packages(&dst, &packages)
            .await
            .map_err(explain)?;
        let mut recipes: Vec<_> = crate::index::read_recipes(&dst)
            .await
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.repo != source.name)
            .collect();
        recipes.extend(mine_recipes(ctx, roots.clone()).await?);
        crate::index::write_recipes(&dst, &recipes)
            .await
            .map_err(explain)?;
        r.done(format!("graph: symbols v{sv}, edges v{ev}"));
        let p = source.path.clone();
        if let Ok(head) = blocking(move || crate::git::head_commit(&p)).await? {
            let _ = crate::index::write_repo_meta(&dst, &source.name, &head).await;
            r.note(format!("recorded commit {}", short(&head)));
        }

        let all: HashSet<String> = symbols.iter().map(|s| s.id.render()).collect();
        enrich_all(
            ctx,
            &dst,
            &Arc::new(symbols),
            &Arc::new(edges),
            &all,
            false,
            &roots,
        )
        .await
    })
}

/// Reparse only the files changed since `base`, drop what they owned, re-bind, and rewrite.
async fn index_changes(ctx: &IndexContext, source: &Source, base: &str) -> Result<()> {
    let r = &ctx.report;
    let dst = ctx.graph();
    let (root, name, base_c) = (source.path.clone(), source.name.clone(), base.to_string());
    let (head, cs) = blocking(move || -> Result<_> {
        let explain =
            |e: anyhow::Error| anyhow::anyhow!(crate::storage::explain(&format!("{e:#}")));
        let head = crate::git::head_commit(&root).map_err(explain)?;
        let cs = crate::git::changed_files(&root, &base_c).map_err(explain)?;
        Ok((head, cs))
    })
    .await??;
    r.step(format!(
        "Incremental {} → {}: {} changed ({}+ {}~ {}-)",
        short(base),
        short(&head),
        cs.total(),
        cs.added.len(),
        cs.modified.len(),
        cs.deleted.len()
    ));
    if cs.total() == 0 {
        let _ = crate::index::write_repo_meta(&dst, &name, &head).await;
        r.done("index already up to date");
        return Ok(());
    }
    let (prior_syms, prior_edges) = crate::index::read_graph(&dst)
        .await
        .with_context(|| format!("no prior index at {dst}; run a full index first"))?;
    let prior_facts = crate::index::read_facts(&dst).await.ok().flatten();
    if prior_facts.is_none() {
        r.warn("index has no stored facts; run a full index for exact edges");
    }
    let root = source.path.clone();
    let (symbols, edges, facts, before, parsed) = blocking(move || {
        let dropped: HashSet<String> = cs.to_drop().cloned().collect();
        let to_parse: Vec<String> = cs.to_parse().cloned().collect();
        let newout = crate::parse::parse_files(&root, &name, &to_parse);
        let file_of: HashMap<String, String> = prior_syms
            .iter()
            .filter(|s| s.repo.0 == name)
            .map(|s| (s.id.render(), s.file_path.clone()))
            .collect();
        let before = prior_syms.len();
        let mut symbols: Vec<Symbol> = prior_syms
            .into_iter()
            .filter(|s| !(s.repo.0 == name && dropped.contains(&s.file_path)))
            .collect();
        let mut edges: Vec<Edge> = prior_edges
            .into_iter()
            .filter(|e| {
                file_of
                    .get(&e.src.render())
                    .is_none_or(|f| !dropped.contains(f))
            })
            .collect();
        symbols.extend(newout.symbols);
        edges.extend(newout.edges);
        let facts = match prior_facts {
            Some(prior) => {
                edges.retain(|e| e.kind == EdgeKind::Contains);
                let mut all: Vec<_> = prior
                    .into_iter()
                    .filter(|f| {
                        f.repo != name || !(dropped.contains(&f.path) || to_parse.contains(&f.path))
                    })
                    .collect();
                all.extend(newout.facts);
                all
            }
            None => newout.facts,
        };
        symbols.retain(|s| s.kind != SymbolKind::Contract);
        let resolved = crate::resolve::resolve(&symbols, &edges, &facts);
        symbols.extend(resolved.symbols);
        (symbols, resolved.edges, facts, before, to_parse.len())
    })
    .await?;
    let explain = |e: anyhow::Error| {
        anyhow::anyhow!(
            "write failed: {}",
            crate::storage::explain(&format!("{e:#}"))
        )
    };
    crate::index::write_graph(&dst, &symbols, &edges)
        .await
        .map_err(explain)?;
    crate::index::write_facts(&dst, &facts)
        .await
        .map_err(explain)?;
    let _ = crate::index::write_repo_meta(&dst, &source.name, &head).await;
    r.done(format!(
        "reparsed {parsed} files → symbols {before}→{}, edges {}",
        symbols.len(),
        edges.len()
    ));
    Ok(())
}

/// The optional steps, each only when the settings ask for it; LLM steps need an LLM.
async fn enrich_all(
    ctx: &IndexContext,
    dst: &str,
    symbols: &Arc<Vec<Symbol>>,
    edges: &Arc<Vec<Edge>>,
    stale: &HashSet<String>,
    incremental: bool,
    roots: &[(String, PathBuf)],
) -> Result<()> {
    let s = &ctx.settings;
    if s.embed {
        embed(ctx, dst, symbols, stale, incremental).await?;
    }
    let wanted = s.enrich || s.flows || s.guide;
    let Some(llm) = ctx.llm.clone().filter(|_| wanted) else {
        if wanted {
            ctx.report.warn(
                "summaries, style guides and walkthroughs skipped: no LLM key (set one in the settings, or OPENAI_API_KEY)",
            );
        }
        return Ok(());
    };
    if s.enrich {
        summaries(ctx, &llm, dst, symbols, edges, stale, incremental).await?;
    }
    if s.guide {
        style_guides(ctx, &llm, dst, symbols, edges, roots).await?;
    }
    if s.flows {
        walkthroughs(ctx, &llm, dst, symbols, edges).await?;
    }
    Ok(())
}

/// Search embeddings, reusing the stored vector of every symbol whose code did not change.
async fn embed(
    ctx: &IndexContext,
    dst: &str,
    symbols: &Arc<Vec<Symbol>>,
    stale: &HashSet<String>,
    incremental: bool,
) -> Result<()> {
    ctx.check()?;
    let prior: HashMap<String, Vec<f32>> = if incremental {
        crate::index::read_search_vectors(dst)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, v)| (id.render(), v))
            .collect()
    } else {
        HashMap::new()
    };
    ctx.report.step("Building search index");
    let (model, syms, stale) = (
        ctx.settings.embed_model.clone(),
        symbols.clone(),
        stale.clone(),
    );
    let (rows, reused, embedded) = blocking(move || -> Result<_> {
        let embedder = crate::embed::Embedder::load(&model).context("embedding model")?;
        let mut rows: Vec<(GlobalSymbolId, String, Vec<f32>)> = Vec::new();
        let mut to_embed: Vec<&Symbol> = Vec::new();
        for s in syms.iter().filter(|s| !matches!(s.kind, SymbolKind::File)) {
            match prior
                .get(&s.id.render())
                .filter(|_| !stale.contains(&s.id.render()))
            {
                Some(v) => rows.push((s.id.clone(), crate::embed::symbol_text(s), v.clone())),
                None => to_embed.push(s),
            }
        }
        let reused = rows.len();
        let texts: Vec<String> = to_embed
            .iter()
            .map(|s| crate::embed::symbol_text(s))
            .collect();
        for (s, v) in to_embed.iter().zip(embedder.embed(&texts)) {
            rows.push((s.id.clone(), crate::embed::symbol_text(s), v));
        }
        Ok((rows, reused, to_embed.len()))
    })
    .await??;
    ctx.report.note(format!(
        "embedding {embedded} symbols + building BM25 index"
    ));
    let v = crate::index::write_search_table(dst, &rows)
        .await
        .context("write search table failed")?;
    ctx.report.done(format!(
        "search index v{v}: {} symbols ({reused} reused, {embedded} embedded) + BM25 FTS index",
        rows.len()
    ));
    Ok(())
}

/// Summaries of the most-used definitions, saved every `CHECKPOINT` and reused while unchanged.
async fn summaries(
    ctx: &IndexContext,
    llm: &Arc<crate::llm::LlmClient>,
    dst: &str,
    symbols: &Arc<Vec<Symbol>>,
    edges: &Arc<Vec<Edge>>,
    stale: &HashSet<String>,
    incremental: bool,
) -> Result<()> {
    const CHECKPOINT: usize = 500;
    let r = &ctx.report;
    r.step("Enriching symbols (LLM)");
    let (syms, edgs, top) = (symbols.clone(), edges.clone(), ctx.settings.max_enrich);
    let defs: Vec<Symbol> = blocking(move || {
        let g = crate::graph::CodeGraph::build(&syms, &edgs);
        let mut defs: Vec<&Symbol> = syms
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SymbolKind::Function
                        | SymbolKind::Method
                        | SymbolKind::Class
                        | SymbolKind::Interface
                        | SymbolKind::Struct
                        | SymbolKind::Enum
                )
            })
            .collect();
        // Most-used first, ties by id, so a limit always picks the same symbols.
        defs.sort_by_cached_key(|s| {
            let id = s.id.render();
            (std::cmp::Reverse(g.dependents_count(&id)), id)
        });
        defs.into_iter().take(top).cloned().collect()
    })
    .await?;
    let prior: HashMap<String, (String, Vec<String>)> = if incremental {
        crate::index::read_enrichment(dst)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, s, q)| (id.render(), (s, q)))
            .collect()
    } else {
        HashMap::new()
    };
    let mut rows: Vec<TextRow> = Vec::new();
    let mut todo: Vec<&Symbol> = Vec::new();
    for s in &defs {
        let rid = s.id.render();
        match prior.get(&rid).filter(|_| !stale.contains(&rid)) {
            Some((sum, q)) => rows.push((s.id.clone(), sum.clone(), q.clone())),
            None => todo.push(s),
        }
    }
    let reused = rows.len();
    if !todo.is_empty() {
        r.warn("Summaries send symbol names, signatures and file paths to the LLM.");
        r.note(format!(
            "{} symbols to summarise ({reused} reused)",
            todo.len()
        ));
    }
    let total = todo.len() as u64;
    let mut done = 0u64;
    let mut generated = 0usize;
    for chunk in todo.chunks(CHECKPOINT) {
        let out = run_llm(
            ctx,
            chunk
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.signature.clone().unwrap_or_default(),
                        s.file_path.clone(),
                    )
                })
                .collect(),
            |(n, sig, path)| {
                let llm = llm.clone();
                async move { llm.enrich_symbol(&n, &sig, &path).await.ok() }
            },
            |d| {
                done += 1;
                let _ = d;
                r.progress("enriching symbols", done, total, Some(usage(ctx, llm)));
            },
        )
        .await;
        let cancelled = out.is_err();
        for (s, e) in chunk.iter().zip(out.unwrap_or_else(|v| v)) {
            if let Some(e) = e {
                rows.push((s.id.clone(), e.summary, e.queries));
                generated += 1;
            }
        }
        if cancelled || chunk.len() == CHECKPOINT {
            crate::index::write_enrichment(dst, &rows)
                .await
                .context("saving summaries")?;
        }
        if cancelled {
            return Err(Cancelled.into());
        }
    }
    if total > 0 {
        r.note(format!("spent: {}", usage(ctx, llm)));
    }
    let failed = todo.len().saturating_sub(generated);
    let v = crate::index::write_enrichment(dst, &rows)
        .await
        .context("write enrichment failed")?;
    r.done(format!(
        "enrichment v{v}: {} symbols ({reused} reused, {generated} generated{})",
        rows.len(),
        if failed > 0 {
            format!(", {failed} failed")
        } else {
            String::new()
        }
    ));
    Ok(())
}

/// One LLM call per item, `concurrency` at a time; `Err` when cancelled, with the calls that
/// finished before the cancellation in their positions.
async fn run_llm<I, T, F, Fut>(
    ctx: &IndexContext,
    items: Vec<I>,
    call: F,
    mut each: impl FnMut(&Option<T>),
) -> std::result::Result<Vec<Option<T>>, Vec<Option<T>>>
where
    F: Fn(I) -> Fut,
    Fut: std::future::Future<Output = Option<T>> + Send + 'static,
    T: Send + 'static,
{
    let mut out = Vec::with_capacity(items.len());
    let mut stream =
        futures::stream::iter(items.into_iter().map(call)).buffered(ctx.settings.llm_concurrency);
    loop {
        tokio::select! {
            _ = ctx.cancel.cancelled() => return Err(out),
            next = stream.next() => match next {
                Some(r) => { each(&r); out.push(r); }
                None => return Ok(out),
            },
        }
    }
}

fn usage(ctx: &IndexContext, llm: &crate::llm::LlmClient) -> Usage {
    let (i, o) = llm.token_usage();
    Usage::priced(i, o, ctx.settings.prices)
}

/// A style guide per repository, reused while its input (the measured evidence) is unchanged.
async fn style_guides(
    ctx: &IndexContext,
    llm: &Arc<crate::llm::LlmClient>,
    dst: &str,
    symbols: &Arc<Vec<Symbol>>,
    edges: &Arc<Vec<Edge>>,
    roots: &[(String, PathBuf)],
) -> Result<()> {
    let r = &ctx.report;
    r.step("Style guides (per repo)");
    r.warn("Style guides send code samples and configuration to the LLM.");
    let facts = crate::index::read_facts(dst).await.ok().flatten();
    let stored_keys = read_keys(dst, "guide_keys").await;
    let mut guides: Vec<(String, String)> = crate::index::read_style_guide(dst)
        .await
        .unwrap_or_default();
    let mut keys: Vec<(String, String)> = stored_keys.clone().into_iter().collect();
    let mut reused = 0usize;
    let (syms, edgs, roots_owned) = (symbols.clone(), edges.clone(), roots.to_vec());
    let per_repo = blocking(move || {
        let conventions = match facts {
            Some(f) => crate::conventions::mine(&syms, &edgs, &f),
            None => Vec::new(),
        };
        roots_owned
            .into_iter()
            .filter_map(|(repo, root)| {
                let rsyms: Vec<&Symbol> = syms.iter().filter(|s| s.id.package == repo).collect();
                if rsyms.is_empty() {
                    return None;
                }
                let redges: Vec<Edge> = edgs
                    .iter()
                    .filter(|e| e.src.package == repo)
                    .cloned()
                    .collect();
                let rconv: Vec<_> = conventions
                    .iter()
                    .filter(|c| c.repo == repo)
                    .cloned()
                    .collect();
                let sections = crate::styleguide::build_sections(&root, &rsyms, &redges, &rconv);
                (!sections.is_empty()).then_some((repo, sections))
            })
            .collect::<Vec<_>>()
    })
    .await?;
    for (repo, sections) in per_repo {
        ctx.check()?;
        let key = fingerprint(
            sections
                .iter()
                .map(|s| format!("{}\0{}\0{}", s.title, s.guidance, s.evidence)),
        );
        if stored_keys.get(&repo) == Some(&key) && guides.iter().any(|(g, _)| *g == repo) {
            reused += 1;
            continue;
        }
        let total = sections.len() as u64;
        let mut done = 0u64;
        let bodies = run_llm(
            ctx,
            sections
                .iter()
                .map(|sec| {
                    (
                        sec.title.clone(),
                        sec.guidance.clone(),
                        sec.evidence.clone(),
                    )
                })
                .collect(),
            |(title, guidance, evidence)| {
                let (llm, repo) = (llm.clone(), repo.clone());
                async move {
                    llm.guide_section(&repo, &title, &guidance, &evidence)
                        .await
                        .ok()
                }
            },
            |_| {
                done += 1;
                r.progress(
                    &format!("{repo}: guide sections"),
                    done,
                    total,
                    Some(usage(ctx, llm)),
                );
            },
        )
        .await
        .map_err(|_| Cancelled)?;
        let mut doc = String::new();
        for (sec, body) in sections.iter().zip(bodies) {
            let body = body.unwrap_or_default();
            let t = body.trim();
            if t.is_empty() || t.eq_ignore_ascii_case("No consistent convention observed.") {
                continue;
            }
            doc.push_str(&format!("## {}\n\n{}\n\n", sec.title, t));
        }
        if doc.is_empty() {
            r.warn(format!("{repo}: no sections generated"));
            continue;
        }
        doc.push_str("\n_AI-generated from the codebase; review before enforcing._\n");
        r.done(format!(
            "{repo}: style guide ({} sections)",
            doc.matches("\n## ").count() + 1
        ));
        guides.retain(|(g, _)| *g != repo);
        guides.push((repo.clone(), doc));
        keys.retain(|(g, _)| *g != repo);
        keys.push((repo, key));
        crate::index::write_style_guide(dst, &guides)
            .await
            .context("saving the style guide")?;
        write_keys(dst, "guide_keys", &keys)
            .await
            .context("saving the style guide")?;
    }
    if reused > 0 {
        r.note(format!("{reused} unchanged style guide(s) reused"));
    }
    r.note(format!("spent: {}", usage(ctx, llm)));
    Ok(())
}

/// Walkthroughs of the flows reaching furthest, reused while their trace is unchanged and saved
/// every `CHECKPOINT`.
async fn walkthroughs(
    ctx: &IndexContext,
    llm: &Arc<crate::llm::LlmClient>,
    dst: &str,
    symbols: &Arc<Vec<Symbol>>,
    edges: &Arc<Vec<Edge>>,
) -> Result<()> {
    const CHECKPOINT: usize = 25;
    let r = &ctx.report;
    r.step("Narrating flows (LLM)");
    let (syms, edgs, top) = (symbols.clone(), edges.clone(), ctx.settings.max_flows);
    let prep: Vec<(GlobalSymbolId, String, String, String)> = blocking(move || {
        let g = crate::graph::CodeGraph::build(&syms, &edgs);
        let by_id: HashMap<String, &Symbol> = syms.iter().map(|s| (s.id.render(), s)).collect();
        g.entry_points(top)
            .into_iter()
            .filter_map(|(idx, _)| {
                let s = by_id.get(&g.node(idx).id)?;
                let trace = g
                    .flow(idx, 6)
                    .iter()
                    .map(|h| {
                        format!(
                            "d{} {} {}:{} {}",
                            h.depth, h.via, h.node.repo, h.node.name, h.node.location
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Some((
                    s.id.clone(),
                    s.name.clone(),
                    s.signature.clone().unwrap_or_default(),
                    trace,
                ))
            })
            .collect()
    })
    .await?;
    if prep.is_empty() {
        r.note("flows: no multi-step flows found to narrate");
        return Ok(());
    }
    r.warn("Walkthroughs send call traces to the LLM.");
    let key = |name: &str, sig: &str, trace: &str| fingerprint([format!("{name}\0{sig}\0{trace}")]);
    let stored_keys = read_keys(dst, "flow_keys").await;
    let stored: HashMap<String, (String, Vec<String>)> = crate::index::read_flows(dst)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, n, q)| (id.render(), (n, q)))
        .collect();
    let mut rows: Vec<TextRow> = Vec::new();
    let mut keys: Vec<(String, String)> = Vec::new();
    let mut todo = Vec::new();
    for p @ (id, name, sig, trace) in &prep {
        let (rid, k) = (id.render(), key(name, sig, trace));
        match stored
            .get(&rid)
            .filter(|_| stored_keys.get(&rid) == Some(&k))
        {
            Some((n, q)) => {
                rows.push((id.clone(), n.clone(), q.clone()));
                keys.push((rid, k));
            }
            None => todo.push(p),
        }
    }
    let reused = rows.len();
    r.note(format!(
        "{} flows to walk through ({reused} reused)",
        todo.len()
    ));
    let total = todo.len() as u64;
    let mut done = 0u64;
    for chunk in todo.chunks(CHECKPOINT) {
        let out = run_llm(
            ctx,
            chunk
                .iter()
                .map(|(_, name, sig, trace)| (name.clone(), sig.clone(), trace.clone()))
                .collect(),
            |(name, sig, trace)| {
                let llm = llm.clone();
                async move { llm.narrate_flow(&name, &sig, &trace).await.ok() }
            },
            |_| {
                done += 1;
                r.progress("narrating flows", done, total, Some(usage(ctx, llm)));
            },
        )
        .await;
        let cancelled = out.is_err();
        for ((id, name, sig, trace), res) in chunk.iter().zip(out.unwrap_or_else(|v| v)) {
            if let Some((narr, q)) = res {
                keys.push((id.render(), key(name, sig, trace)));
                rows.push((id.clone(), narr, q));
            }
        }
        if cancelled || chunk.len() == CHECKPOINT {
            crate::index::write_flows(dst, &rows)
                .await
                .context("saving walkthroughs")?;
            write_keys(dst, "flow_keys", &keys)
                .await
                .context("saving walkthroughs")?;
        }
        if cancelled {
            return Err(Cancelled.into());
        }
    }
    r.note(format!("spent: {}", usage(ctx, llm)));
    let v = crate::index::write_flows(dst, &rows)
        .await
        .context("write flows failed")?;
    write_keys(dst, "flow_keys", &keys)
        .await
        .context("write flows failed")?;
    r.done(format!(
        "flows v{v}: {} narrated ({reused} reused, {} written)",
        rows.len(),
        rows.len() - reused
    ));
    if let Some((_, narr, _)) = rows.first() {
        r.note(format!(
            "e.g. {}",
            narr.chars().take(120).collect::<String>()
        ));
    }
    Ok(())
}

fn fingerprint(parts: impl IntoIterator<Item = String>) -> String {
    let mut h = blake3::Hasher::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update(b"\0\0");
    }
    h.finalize().to_hex().to_string()
}

async fn read_keys(dst: &str, table: &str) -> HashMap<String, String> {
    crate::index::read_keys(dst, table)
        .await
        .unwrap_or_default()
}

async fn write_keys(dst: &str, table: &str, keys: &[(String, String)]) -> Result<u64> {
    crate::index::write_keys(dst, table, keys).await
}

/// Symbols whose repository changed files since the commits recorded last time.
async fn stale_ids(
    dst: &str,
    roots: Vec<(String, PathBuf)>,
    symbols: &[Symbol],
) -> Result<HashSet<String>> {
    let prior = crate::index::read_repo_commits(dst)
        .await
        .unwrap_or_default();
    let changed = blocking(move || {
        roots
            .into_iter()
            .map(|(name, p)| {
                let set = match (prior.get(&name), crate::git::head_commit(&p)) {
                    (Some(base), Ok(_)) => crate::git::changed_files(&p, base).ok().map(|cs| {
                        cs.added
                            .into_iter()
                            .chain(cs.modified)
                            .collect::<HashSet<_>>()
                    }),
                    _ => None,
                };
                (name, set)
            })
            .collect::<HashMap<_, _>>()
    })
    .await?;
    Ok(symbols
        .iter()
        .filter(|s| match changed.get(&s.id.package) {
            Some(Some(files)) => files.contains(&s.file_path),
            _ => true,
        })
        .map(|s| s.id.render())
        .collect())
}

/// Manifests and lockfiles of each repository.
async fn scan_packages(
    ctx: &IndexContext,
    roots: Vec<(String, PathBuf)>,
) -> Result<Vec<crate::manifests::PackageRow>> {
    let rows = blocking(move || {
        roots
            .iter()
            .flat_map(|(name, root)| crate::manifests::scan(root, name))
            .collect::<Vec<_>>()
    })
    .await?;
    let deps = crate::manifests::internal(&rows);
    if !deps.is_empty() {
        let skewed = deps.iter().filter(|d| d.skewed()).count();
        ctx.report.done(format!(
            "internal packages: {} dependency link(s), {skewed} pinned to a different version than their publisher",
            deps.len()
        ));
    }
    Ok(rows)
}

/// Change recipes from each repository's recent history.
async fn mine_recipes(
    ctx: &IndexContext,
    roots: Vec<(String, PathBuf)>,
) -> Result<Vec<crate::conventions::Convention>> {
    let out = blocking(move || {
        roots
            .iter()
            .filter_map(|(name, root)| {
                crate::git::history(root, 500)
                    .ok()
                    .map(|h| crate::recipes::mine(name, &h))
            })
            .flatten()
            .collect::<Vec<_>>()
    })
    .await?;
    if !out.is_empty() {
        ctx.report
            .done(format!("recipes: {} from commit history", out.len()));
    }
    Ok(out)
}

/// Attach SCIP to one parsed repository; a note when indexes were found.
fn attach_scip(
    mode: crate::scip::Mode,
    root: &Path,
    name: &str,
    out: &mut crate::parse::ParseOutput,
) -> Result<Option<String>> {
    let a = crate::scip::ingest(mode, root, name, &out.symbols, &mut out.facts)?;
    Ok((a.documents > 0).then(|| {
        format!(
            "{name}: SCIP {} index(es), {} files, {} defs, {} refs",
            a.indexes, a.documents, a.definitions, a.references
        )
    }))
}

fn report_resolution(ctx: &IndexContext, resolved: &crate::resolve::Resolved) {
    let r = &ctx.report;
    if !resolved.symbols.is_empty() {
        let mut by_type: BTreeMap<&str, usize> = BTreeMap::new();
        for c in &resolved.symbols {
            *by_type
                .entry(c.name.split(':').next().unwrap_or_default())
                .or_default() += 1;
        }
        let parts: Vec<String> = by_type.iter().map(|(t, n)| format!("{t} {n}")).collect();
        r.done(format!(
            "contracts: {} ({}), {} participant edges",
            resolved.symbols.len(),
            parts.join(", "),
            resolved.stats.contract_edges
        ));
    }
    let s = &resolved.stats;
    r.done(format!(
        "resolved: {} imports, {} calls, {} inherits/implements; {} cross-repo edges{}",
        s.resolved_imports,
        s.resolved_calls,
        s.type_edges,
        s.cross_repo_edges,
        if s.scip_answers > 0 {
            format!(" ({} answered by SCIP)", s.scip_answers)
        } else {
            String::new()
        }
    ));
    r.note(format!(
        "unresolved: {} imports, {} calls (mostly third-party or dynamic)",
        s.unresolved_imports, s.unresolved_calls
    ));
    let mut top: Vec<_> = resolved.unresolved_modules.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let top: Vec<String> = top
        .iter()
        .take(12)
        .map(|(m, n)| format!("{m} {n}"))
        .collect();
    if !top.is_empty() {
        r.note(format!("top unresolved: {}", top.join(", ")));
    }
}

/// Cross-repository import targets, most depended-on first.
fn dependencies(edges: &[Edge]) -> Vec<Dependency> {
    let mut targets: BTreeMap<String, (String, BTreeSet<String>, usize)> = BTreeMap::new();
    for e in edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Imports && e.cross_repo)
    {
        let t = targets
            .entry(e.dst.descriptor.clone())
            .or_insert_with(|| (e.dst.package.clone(), BTreeSet::new(), 0));
        t.1.insert(e.src.package.clone());
        t.2 += 1;
    }
    let mut out: Vec<Dependency> = targets
        .into_iter()
        .map(|(descriptor, (defined_in, repos, refs))| Dependency {
            descriptor,
            defined_in,
            repos,
            refs,
        })
        .collect();
    out.sort_by(|a, b| b.repos.len().cmp(&a.repos.len()).then(b.refs.cmp(&a.refs)));
    out
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}
