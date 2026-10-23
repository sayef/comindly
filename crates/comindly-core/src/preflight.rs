//! Preflight: before a change lands, what the team already knows about the code it touches —
//! recorded constraints and dead ends, contracts other services rely on, blast radius, and
//! directories that usually change together but are missing from the change.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use crate::conventions::Convention;
use crate::graph::{CodeGraph, PROVIDES};
use crate::ledger::Entry;
use crate::model::{Edge, Symbol, SymbolKind};

/// A changed file in a repo, with changed line ranges (empty = whole file / unknown).
pub struct Change {
    pub repo: String,
    pub path: String,
    pub lines: Vec<(u32, u32)>,
}

/// The innermost symbols each change touches (files fall back to their file symbol).
pub fn touched(symbols: &[Symbol], changes: &[Change]) -> Vec<String> {
    let mut out = Vec::new();
    for c in changes {
        let in_file: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.repo.0 == c.repo && s.file_path == c.path)
            .collect();
        let defs: Vec<&&Symbol> = in_file
            .iter()
            .filter(|s| s.kind != SymbolKind::File)
            .collect();
        let mut hit = false;
        for (a, b) in &c.lines {
            // Innermost: the narrowest definition overlapping the range.
            if let Some(s) = defs
                .iter()
                .filter(|s| s.range.start.line <= *b && *a <= s.range.end.line)
                .min_by_key(|s| s.range.end.line - s.range.start.line)
            {
                out.push(s.id.render());
                hit = true;
            }
        }
        if !hit {
            if let Some(f) = in_file.iter().find(|s| s.kind == SymbolKind::File) {
                out.push(f.id.render());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn report(
    graph: &CodeGraph,
    symbols: &[Symbol],
    edges: &[Edge],
    entries: &[Entry],
    recipes: &[Convention],
    findings: &[crate::checks::Finding],
    changes: &[Change],
) -> String {
    let ids = touched(symbols, changes);
    let by_id: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.render(), s)).collect();
    let files: BTreeSet<(&str, &str)> = changes
        .iter()
        .map(|c| (c.repo.as_str(), c.path.as_str()))
        .collect();
    let mut o = format!(
        "## Preflight — {} file(s), {} symbol(s) touched\n",
        files.len(),
        ids.len()
    );

    // Recorded reasoning about the touched code and its containers.
    let mut scope: Vec<String> = Vec::new();
    for id in &ids {
        if let Some((_, s)) = graph.scope_of(id) {
            scope.extend(s);
        }
    }
    let known = crate::ledger::about(entries, &scope);
    let live: Vec<&&Entry> = known
        .iter()
        .filter(|e| {
            !matches!(
                e.status,
                crate::ledger::Status::Retracted | crate::ledger::Status::Superseded
            )
        })
        .collect();
    o.push_str("\n### Recorded constraints & dead ends\n");
    if live.is_empty() {
        o.push_str("_none recorded for this code_\n");
    }
    for e in live {
        let _ = writeln!(o, "- {}", crate::ledger::line(e));
    }

    let changed: Vec<String> = files.iter().map(|(r, p)| format!("{r}/{p}")).collect();
    let checks = crate::checks::markdown(findings, Some(&changed));
    if !checks.is_empty() {
        o.push_str("\n### Constraint checks failing in changed files\n");
        o.push_str(&checks);
    }

    // Contracts provided by touched symbols, with their consumers.
    o.push_str("\n### Contracts you are changing\n");
    let mut any = false;
    for e in edges.iter().filter(|e| PROVIDES.contains(&e.kind)) {
        let src = e.src.render();
        if !ids.contains(&src) {
            continue;
        }
        let Some(c) = by_id.get(&e.dst.render()) else {
            continue;
        };
        let users: BTreeSet<String> = edges
            .iter()
            .filter(|u| u.dst == e.dst && !PROVIDES.contains(&u.kind))
            .filter_map(|u| by_id.get(&u.src.render()))
            .map(|s| format!("{}:{}", s.repo.0, s.name))
            .collect();
        any = true;
        let _ = writeln!(
            o,
            "- `{}` ({} by `{}`) — used by {}",
            c.name,
            e.kind,
            by_id.get(&src).map_or("?", |s| s.name.as_str()),
            if users.is_empty() {
                "nothing indexed".to_string()
            } else {
                users.into_iter().collect::<Vec<_>>().join(", ")
            }
        );
    }
    if !any {
        o.push_str("_none_\n");
    }

    // Blast radius.
    o.push_str("\n### Blast radius\n");
    let mut lines = Vec::new();
    for id in &ids {
        let Some(idx) = graph.lookup(id) else {
            continue;
        };
        let hops = graph.ripple(idx, 3);
        if hops.is_empty() {
            continue;
        }
        let me = by_id.get(id).map_or("", |s| s.repo.0.as_str());
        let repos: BTreeSet<&str> = hops
            .iter()
            .map(|h| h.node.repo.as_str())
            .filter(|r| *r != me && *r != crate::resolve::CONTRACTS)
            .collect();
        lines.push((
            hops.len(),
            format!(
                "- `{}`: {} dependents{}",
                by_id.get(id).map_or(id.as_str(), |s| s.name.as_str()),
                hops.len(),
                if repos.is_empty() {
                    String::new()
                } else {
                    format!(
                        " — other repos: {}",
                        repos.into_iter().collect::<Vec<_>>().join(", ")
                    )
                }
            ),
        ));
    }
    lines.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    if lines.is_empty() {
        o.push_str("_no indexed dependents_\n");
    }
    for (_, l) in lines.into_iter().take(10) {
        let _ = writeln!(o, "{l}");
    }

    // Recipes: companions this kind of change usually has, missing here.
    let dirs: BTreeSet<(String, String)> = changes
        .iter()
        .map(|c| {
            (
                c.repo.clone(),
                c.path.rsplit_once('/').map_or(".", |(d, _)| d).to_string(),
            )
        })
        .collect();
    let mut hints = Vec::new();
    for r in recipes.iter().filter(|r| r.dimension == "recipes") {
        let Some(anchor) = r.key.rsplit(':').next() else {
            continue;
        };
        if !dirs.contains(&(r.repo.clone(), anchor.to_string())) {
            continue;
        }
        let missing: Vec<&str> = r
            .generic
            .split('`')
            .skip(3)
            .step_by(2)
            .map(|d| d.trim_end_matches('/'))
            .filter(|d| !dirs.iter().any(|(repo, x)| repo == &r.repo && x == d))
            .collect();
        if !missing.is_empty() {
            hints.push(format!(
                "- changes in `{anchor}/` usually also touch {} — not in this change (e.g. {})",
                missing
                    .iter()
                    .map(|d| format!("`{d}/`"))
                    .collect::<Vec<_>>()
                    .join(", "),
                r.exemplars.first().cloned().unwrap_or_default()
            ));
        }
    }
    o.push_str("\n### Usually changed together\n");
    if hints.is_empty() {
        o.push_str("_nothing missing_\n");
    }
    for h in hints {
        let _ = writeln!(o, "{h}");
    }
    o
}
