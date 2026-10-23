//! Flows across repositories, and the source lines behind a `path:line`.

use std::path::Path;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

use super::{Cur, Reply};
use crate::api;

/// Every flow worth showing: entry points that reach furthest, plus any the index narrated.
pub(super) async fn list(Cur(s): Cur) -> Json<Vec<api::FlowRow>> {
    let extras = s.extras().await;
    let rows = s
        .flow_list
        .get_or_init(|| async {
            let g = &s.data.graph;
            let mut starts: Vec<_> = g.entry_points(60).into_iter().map(|(i, _)| i).collect();
            for id in extras.flows.keys() {
                if let Some(i) = g.lookup(id).filter(|i| !starts.contains(i)) {
                    starts.push(i);
                }
            }
            starts
                .into_iter()
                .map(|i| {
                    let entry = g.node(i);
                    let steps = g.flow(i, 8);
                    // Repositories in the order the flow first reaches them.
                    let mut repos: Vec<String> = vec![entry.repo.clone()];
                    for s in steps.iter().filter(|s| s.node.kind != "Contract") {
                        if !repos.contains(&s.node.repo) {
                            repos.push(s.node.repo.clone());
                        }
                    }
                    let contracts = steps
                        .iter()
                        .filter(|s| s.node.kind == "Contract")
                        .map(|s| s.node.name.clone())
                        .collect();
                    let narration = extras.flows.get(&entry.id).map(|(n, _)| n.as_str());
                    api::FlowRow {
                        steps: steps.len(),
                        repos,
                        contracts,
                        summary: narration.and_then(|n| n.split("\n\n").next().map(String::from)),
                        narrated: narration.is_some(),
                        entry,
                    }
                })
                .collect()
        })
        .await;
    Json(rows.clone())
}

/// One flow: its walkthrough (when the index has one) and every step, each with the step before it.
pub(super) async fn one(Cur(s): Cur, Query(q): Query<api::FlowQuery>) -> Reply<api::FlowDetail> {
    let g = &s.data.graph;
    let Some(i) = g.lookup(&q.id) else {
        return Err(super::failure(StatusCode::NOT_FOUND, "no such symbol"));
    };
    let entry = g.node(i);
    let extras = s.extras().await;
    let walk = extras.flows.get(&entry.id).cloned();
    Ok(Json(api::FlowDetail {
        entry,
        narration: walk.as_ref().map(|(n, _)| n.clone()),
        questions: walk.map(|(_, q)| q),
        steps: g.flow(i, 8),
    }))
}

/// The lines around `path:line` in this machine's copy of `repo` (comindly's checkout of its default branch when there is one).
pub(super) async fn source(
    State(app): State<super::Shared>,
    Query(q): Query<api::SourceQuery>,
) -> Reply<api::SourceLines> {
    let repos = app.repos();
    let run = || -> anyhow::Result<api::SourceLines> {
        let (path, line) = q.at.rsplit_once(':').unwrap_or((q.at.as_str(), "1"));
        let line: usize = line.parse().unwrap_or(1).max(1);
        let entry = repos
            .iter()
            .find(|r| crate::worktree::name(Path::new(&r.path)) == q.repo)
            .ok_or_else(|| anyhow::anyhow!("{} has no folder on this machine", q.repo))?;
        let repo = Path::new(&entry.path);
        let root = crate::worktree::checkout_dir(&crate::config::cache_dir(), repo)
            .unwrap_or_else(|| repo.to_path_buf());
        let root = std::fs::canonicalize(&root)?;
        let file = std::fs::canonicalize(root.join(path))?;
        anyhow::ensure!(file.starts_with(&root), "{path} is outside the repository");
        let text = std::fs::read_to_string(&file)?;
        let from = line.saturating_sub(q.around + 1);
        let lines = text
            .lines()
            .enumerate()
            .skip(from)
            .take(q.around * 2 + 1)
            .map(|(n, l)| api::SourceLine {
                n: n + 1,
                text: l.to_string(),
            })
            .collect();
        Ok(api::SourceLines {
            path: path.to_string(),
            line,
            lines,
        })
    };
    run().map(Json).map_err(super::bad)
}
