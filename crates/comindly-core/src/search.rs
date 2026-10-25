//! Hybrid code search: native LanceDB retrieval (BM25 + vector via RRF) followed by comindly's
//! code-aware rerank — definition/exact-name boosts, structured path penalties, LLM-query
//! recall boost, and a dependency-graph **centrality** signal no pure search tool has.
//!
//! Shared by the `comindly search` CLI and the `search` MCP tool.

use std::collections::HashMap;

use crate::embed::{rank, Embedder};
use crate::graph::CodeGraph;
use crate::model::{Symbol, SymbolKind};

/// Per-symbol LLM enrichment keyed by rendered symbol id: `(summary, generated_queries)`.
pub type Enrichment = HashMap<String, (String, Vec<String>)>;

/// One ranked search result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    /// `path:line`.
    pub location: String,
    pub signature: Option<String>,
    pub score: f32,
    pub deps: usize,
    pub summary: Option<String>,
}

/// Retrieve candidates for `query` through the open `tables` and rank them. `limit` caps results.
pub async fn hybrid(
    tables: &crate::index::Tables,
    by_id: &HashMap<String, Symbol>,
    graph: &CodeGraph,
    enrichment: &Enrichment,
    embedder: &Embedder,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchHit>> {
    let qvec = embedder.embed_query(query);
    let candidates = tables.hybrid_search(query, qvec, 80).await?;
    let mut hits = rank_candidates(candidates, by_id, graph, enrichment, query);
    hits.truncate(limit);
    Ok(hits)
}

/// Blocking variant for the CLI (spawns its own runtime under the hood).
pub fn hybrid_blocking(
    uri: &str,
    by_id: &HashMap<String, Symbol>,
    graph: &CodeGraph,
    enrichment: &Enrichment,
    embedder: &Embedder,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<SearchHit>> {
    let qvec = embedder.embed_query(query);
    let candidates = crate::index::hybrid_search_blocking(uri, query, qvec, 80)?;
    let mut hits = rank_candidates(candidates, by_id, graph, enrichment, query);
    hits.truncate(limit);
    Ok(hits)
}

/// Render ranked hits as markdown for handoff to an agent (or `--format md` on the CLI).
pub fn markdown(query: &str, hits: &[SearchHit]) -> String {
    use std::fmt::Write as _;
    let mut o = format!("## Search: \"{query}\" — {} result(s)\n\n", hits.len());
    if hits.is_empty() {
        return o + "_no results (or the index was built with `--no-embed`)_\n";
    }
    for (i, h) in hits.iter().enumerate() {
        let _ = write!(
            o,
            "{}. **{}** _{}_  ·  score {:.3}  ·  `{}`",
            i + 1,
            h.name,
            h.kind,
            h.score,
            h.location,
        );
        if !h.repo.is_empty() {
            let _ = write!(o, "  ·  {}", h.repo);
        }
        let _ = write!(o, "  ·  {} deps", h.deps);
        if let Some(s) = &h.summary {
            let _ = write!(o, "\n   ↳ {s}");
        }
        // Stable handle for zoom/ripple/thread/flow/context_pack.
        let _ = write!(o, "\n   id `{}`", h.id);
        o.push('\n');
    }
    o.push_str("\n_Pass an `id` to `zoom`/`ripple`/`context_pack` to navigate from a hit._\n");
    o
}

/// Apply comindly's code-aware + centrality boosts to native-retrieval candidates `(id, base)`.
pub fn rank_candidates(
    candidates: Vec<(String, f32)>,
    by_id: &HashMap<String, Symbol>,
    graph: &CodeGraph,
    enrichment: &Enrichment,
    query: &str,
) -> Vec<SearchHit> {
    let keywords = rank::query_keywords(query);
    let symbolish = rank::is_symbol_query(query);

    let mut ranked: Vec<SearchHit> = candidates
        .into_iter()
        .filter_map(|(rid, base)| {
            let sym = by_id.get(&rid)?;
            let deps = graph.dependents_count(&rid);
            let enr = enrichment.get(&rid);

            let is_def = matches!(
                sym.kind,
                SymbolKind::Function
                    | SymbolKind::Method
                    | SymbolKind::Class
                    | SymbolKind::Interface
                    | SymbolKind::Struct
                    | SymbolKind::Enum
            );
            let def_boost = if is_def { 1.2 } else { 1.0 };
            let exact_boost = if symbolish && sym.name.eq_ignore_ascii_case(query.trim()) {
                2.5
            } else {
                1.0
            };
            let penalty = rank::path_penalty(&sym.file_path);
            let graph_boost = 1.0 + 0.15 * (1.0 + deps as f32).ln();
            let query_boost = enr.map_or(1.0, |(_, qs)| 1.0 + 0.4 * query_match(&keywords, qs));

            let score = base * def_boost * exact_boost * penalty * graph_boost * query_boost;
            Some(SearchHit {
                id: rid.clone(),
                name: graph
                    .lookup(&rid)
                    .map_or_else(|| sym.name.clone(), |i| graph.node(i).name),
                kind: format!("{:?}", sym.kind),
                repo: sym.repo.0.clone(),
                location: format!("{}:{}", sym.file_path, sym.range.start.line),
                signature: sym.signature.clone(),
                score,
                deps,
                summary: enr.map(|(s, _)| s.clone()),
            })
        })
        .collect();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Collapse near-duplicates (a symbol surfaced by both BM25 and vector, or a duplicated
    // search row): keep the highest-scored hit per unique symbol location.
    let mut seen = std::collections::HashSet::new();
    ranked.retain(|h| seen.insert((h.name.clone(), h.location.clone())));
    ranked
}

/// Max overlap between the user's keywords and any one LLM-generated query (0..1).
fn query_match(keywords: &[String], generated: &[String]) -> f32 {
    if keywords.is_empty() {
        return 0.0;
    }
    generated
        .iter()
        .map(|q| {
            let qk = rank::query_keywords(q);
            keywords.iter().filter(|k| qk.contains(*k)).count() as f32 / keywords.len() as f32
        })
        .fold(0.0, f32::max)
}
