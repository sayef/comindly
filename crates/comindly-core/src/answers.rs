//! What the read commands answer, computed the same way on this machine and on a server, so a
//! command run with `--server` prints what a local run would.

use serde::{Deserialize, Serialize};

use crate::conventions::{Convention, Manual, ORG};
use crate::graph::{CodeGraph, ContractView, Node};
use crate::ledger::Entry;
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

fn in_repos(repo: &str, repos: &[String]) -> bool {
    repos.iter().any(|r| repo.eq_ignore_ascii_case(r))
}
