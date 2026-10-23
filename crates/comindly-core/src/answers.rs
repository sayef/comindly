//! What the read commands answer, computed the same way on this machine and on a server, so a
//! command run with `--server` prints what a local run would.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::{Deserialize, Serialize};

use crate::conventions::{Convention, Manual, ORG};
use crate::graph::{CodeGraph, ContractView, Node};
use crate::ledger::Entry;
use crate::manifests::InternalDep;
use crate::model::{Edge, Symbol};
use crate::search::SearchHit;

/// `find`: symbols whose name, descriptor or path matches, in `repos` when given.
pub fn find(g: &CodeGraph, query: &str, repos: &[String]) -> Vec<Node> {
    // Over-fetch when filtering by repo so a full page survives the filter.
    let mut hits = g.find(query, if repos.is_empty() { 30 } else { 200 });
    if !repos.is_empty() {
        hits.retain(|n| in_repos(&n.repo, repos));
        hits.truncate(30);
    }
    hits
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Search {
    pub hits: Vec<SearchHit>,
    pub markdown: String,
}

/// How many hits to ask the ranker for; more when they are filtered by repo afterwards.
pub fn search_limit(repos: &[String]) -> usize {
    if repos.is_empty() {
        12
    } else {
        60
    }
}

/// `search`: ranked hits, kept to `repos` when given.
pub fn search(query: &str, mut hits: Vec<SearchHit>, repos: &[String]) -> Search {
    if !repos.is_empty() {
        hits.retain(|h| in_repos(&h.repo, repos));
        hits.truncate(12);
    }
    Search {
        markdown: crate::search::markdown(query, &hits),
        hits,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Why {
    pub name: String,
    pub markdown: String,
}

/// `why`: the ledger about `focus` and what it contains.
pub fn why(g: &CodeGraph, entries: &[Entry], focus: &str) -> Option<Why> {
    let (name, ids) = g.scope_of(focus)?;
    let about = crate::ledger::about(entries, &ids);
    Some(Why {
        markdown: crate::ledger::markdown(&format!("why `{name}`"), &about),
        name,
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub markdown: String,
    /// Said instead of a report, when there is nothing to report.
    pub note: Option<String>,
    /// With `check`: a constraint that must hold is violated.
    pub failed: bool,
}

/// `ledger`: the records, or the inbox of those needing attention; with `check`, constraint checks.
pub fn ledger(
    mut entries: Vec<Entry>,
    symbols: &[Symbol],
    edges: &[Edge],
    packages: &[crate::manifests::PackageRow],
    inbox: bool,
    check: bool,
) -> Ledger {
    let findings = crate::checks::run(&entries, symbols, edges, packages);
    if check {
        if findings.is_empty() {
            return Ledger {
                note: Some("no constraint carries a check (add one with --field check=...)".into()),
                ..Default::default()
            };
        }
        return Ledger {
            markdown: format!(
                "## Constraint checks\n\n{}",
                crate::checks::markdown(&findings, None)
            ),
            failed: findings
                .iter()
                .any(|f| f.severity == "must" && !f.violations.is_empty()),
            note: None,
        };
    }
    crate::checks::promote(&mut entries, &findings);
    let shown = crate::ledger::inbox(&entries, inbox);
    let title = if inbox { "Ledger inbox" } else { "Ledger" };
    let mut markdown = format!("{}\n", crate::ledger::markdown(title, &shown));
    let revived = crate::checks::revivals(&entries, packages);
    if !revived.is_empty() {
        markdown.push_str("## Retry conditions met\n\n");
        for (e, why) in revived {
            markdown.push_str(&format!("- {} — {why}\n", crate::ledger::line(e)));
        }
    }
    Ledger {
        markdown,
        ..Default::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub focus: String,
    /// The stored walkthrough and the questions it answers, when one was written.
    pub narration: Option<(String, Vec<String>)>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub depth: usize,
    pub via: String,
    pub name: String,
    pub location: String,
}

/// `flow`: the walkthrough written for `focus`, if any, and its live call trace.
pub fn flow(
    g: &CodeGraph,
    focus: &str,
    narration: impl FnOnce(&str) -> Option<(String, Vec<String>)>,
) -> Option<Flow> {
    let idx = g.lookup(focus)?;
    let narration = g.zoom(idx).focus.and_then(|n| narration(&n.id));
    let steps = g
        .thread(idx, 4)
        .into_iter()
        .map(|h| Step {
            depth: h.depth,
            via: h.via.to_string(),
            name: h.node.name,
            location: h.node.location,
        })
        .collect();
    Some(Flow {
        focus: focus.to_string(),
        narration,
        steps,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contracts {
    /// Every contract, before `kind` and `dangling` narrow the list.
    pub total: usize,
    pub views: Vec<ContractView>,
}

pub fn contracts(g: &CodeGraph, kind: Option<&str>, dangling: bool) -> Contracts {
    Contracts {
        total: g.contracts(None, false).len(),
        views: g.contracts(kind, dangling),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conventions {
    pub conventions: Vec<Convention>,
    pub hidden: Vec<Manual>,
}

/// `conventions`: the conventions in force (measured, edited and the team's) and the hidden ones,
/// for `repo` and the org baseline when a repo is given.
pub fn conventions(all: Vec<Convention>, manual: &[Manual], repo: Option<&str>) -> Conventions {
    let here = |r: &str| repo.is_none_or(|want| r == want || r == ORG);
    Conventions {
        conventions: all.into_iter().filter(|c| here(&c.repo)).collect(),
        hidden: manual
            .iter()
            .filter(|m| m.hidden && here(&m.repo))
            .cloned()
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explore {
    pub symbols: usize,
    pub edges: usize,
    pub dangling: usize,
    /// `None` when no symbol matches the focus.
    pub view: Option<ExploreView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploreView {
    pub focus: Option<Node>,
    pub container: Option<Node>,
    pub callees: Vec<Node>,
    pub callers: Vec<Node>,
    pub importers: Vec<Node>,
    pub members: Vec<Node>,
    /// Dependents within the blast radius, and how many per repo.
    pub dependents: usize,
    pub by_repo: BTreeMap<String, usize>,
    pub budget: usize,
    /// The context pack: symbols to read with their estimated tokens.
    pub pack: Vec<(Node, usize)>,
}

/// Hops the blast radius follows, and the context pack's token budget.
pub const RIPPLE_DEPTH: usize = 4;
pub const PACK_BUDGET: usize = 1500;

/// `explore`: zoom, blast radius and context pack for `focus`.
pub fn explore(g: &CodeGraph, focus: &str) -> Explore {
    let view = g.lookup(focus).map(|idx| {
        let z = g.zoom(idx);
        let hits = g.ripple(idx, RIPPLE_DEPTH);
        let mut by_repo = BTreeMap::new();
        for h in &hits {
            *by_repo.entry(h.node.repo.clone()).or_default() += 1;
        }
        ExploreView {
            focus: z.focus,
            container: z.container,
            callees: z.callees,
            callers: z.callers,
            importers: z.importers,
            members: z.members,
            dependents: hits.len(),
            by_repo,
            budget: PACK_BUDGET,
            pack: g.context_pack(idx, PACK_BUDGET),
        }
    });
    Explore {
        symbols: g.node_count(),
        edges: g.edge_count(),
        dangling: g.dangling_edges,
        view,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub symbols: usize,
    /// Repos in the index; `repos` holds those reported.
    pub indexed: usize,
    /// Asked-for repos the index does not have.
    pub unknown: Vec<String>,
    pub repos: Vec<RepoStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    pub repo: String,
    pub symbols: usize,
    pub edges: usize,
    pub enriched: usize,
    pub kinds: BTreeMap<String, usize>,
}

/// `stats`: per repo (`want`, else all) symbols, edges, kinds and how many have LLM summaries.
pub fn stats(
    symbols: &[Symbol],
    edges: &[Edge],
    enriched: &HashSet<String>,
    want: &[String],
) -> Stats {
    let all: BTreeSet<&str> = symbols.iter().map(|s| s.id.package.as_str()).collect();
    let (known, unknown): (Vec<&String>, Vec<&String>) =
        want.iter().partition(|r| all.contains(r.as_str()));
    let repos: Vec<&str> = if want.is_empty() {
        all.iter().copied().collect()
    } else {
        known.into_iter().map(String::as_str).collect()
    };
    let repos = repos
        .into_iter()
        .map(|repo| {
            let rsyms: Vec<&Symbol> = symbols.iter().filter(|s| s.id.package == repo).collect();
            let mut kinds = BTreeMap::new();
            for s in &rsyms {
                *kinds.entry(format!("{:?}", s.kind)).or_default() += 1;
            }
            RepoStats {
                repo: repo.to_string(),
                symbols: rsyms.len(),
                edges: edges.iter().filter(|e| e.src.package == repo).count(),
                enriched: rsyms
                    .iter()
                    .filter(|s| enriched.contains(&s.id.render()))
                    .count(),
                kinds,
            }
        })
        .collect();
    Stats {
        symbols: symbols.len(),
        indexed: all.len(),
        unknown: unknown.into_iter().cloned().collect(),
        repos,
    }
}

/// `packages`: internal dependencies, only the skewed ones with `skew`.
pub fn packages(deps: Vec<InternalDep>, skew: bool) -> Vec<InternalDep> {
    deps.into_iter().filter(|d| !skew || d.skewed()).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preflight {
    pub markdown: String,
    /// A `must` constraint check fails in a changed file.
    pub blocking: bool,
}

/// `preflight`: what is known about the code `changes` touch; the changes come from git.
pub fn preflight(
    g: &CodeGraph,
    symbols: &[Symbol],
    edges: &[Edge],
    mut entries: Vec<Entry>,
    recipes: &[Convention],
    packages: &[crate::manifests::PackageRow],
    changes: &[crate::preflight::Change],
) -> Preflight {
    let findings = crate::checks::run(&entries, symbols, edges, packages);
    crate::checks::promote(&mut entries, &findings);
    let markdown =
        crate::preflight::report(g, symbols, edges, &entries, recipes, &findings, changes);
    let changed: Vec<String> = changes
        .iter()
        .map(|c| format!("{}/{}", c.repo, c.path))
        .collect();
    let blocking = findings.iter().any(|f| {
        f.severity == "must"
            && f.violations
                .iter()
                .any(|v| changed.contains(&format!("{}/{}", v.repo, v.path)))
    });
    Preflight { markdown, blocking }
}

fn in_repos(repo: &str, repos: &[String]) -> bool {
    repos.iter().any(|r| repo.eq_ignore_ascii_case(r))
}
