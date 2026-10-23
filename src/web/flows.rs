//! Flows across repositories, and the source lines behind a `path:line`.

use std::path::Path;

use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{Conf, Cur};

/// Every flow worth showing: entry points that reach furthest, plus any the index narrated.
pub(super) async fn list(Cur(s): Cur) -> Json<Value> {
    let extras = s.extras().await;
    let v = s
        .flow_list
        .get_or_init(|| async {
            let g = &s.data.graph;
            let mut starts: Vec<_> = g.entry_points(60).into_iter().map(|(i, _)| i).collect();
            for id in extras.flows.keys() {
                if let Some(i) = g.lookup(id).filter(|i| !starts.contains(i)) {
                    starts.push(i);
                }
            }
            let rows: Vec<Value> = starts
                .into_iter()
                .map(|i| {
                    let entry = g.node(i);
                    let steps = g.flow(i, 8);
                    // Repositories in the order the flow first reaches them.
                    let mut repos: Vec<&str> = vec![&entry.repo];
                    for s in steps.iter().filter(|s| s.node.kind != "Contract") {
                        if !repos.contains(&s.node.repo.as_str()) {
                            repos.push(&s.node.repo);
                        }
                    }
                    let contracts: Vec<&str> = steps
                        .iter()
                        .filter(|s| s.node.kind == "Contract")
                        .map(|s| s.node.name.as_str())
                        .collect();
                    let narration = extras.flows.get(&entry.id).map(|(n, _)| n.as_str());
                    json!({
                        "entry": entry,
                        "steps": steps.len(),
                        "repos": repos,
                        "contracts": contracts,
                        "summary": narration.and_then(|n| n.split("\n\n").next()),
                        "narrated": narration.is_some(),
                    })
                })
                .collect();
            json!(rows)
        })
        .await;
    Json(v.clone())
}

#[derive(Deserialize)]
pub(super) struct OneQ {
    id: String,
}

/// One flow: its walkthrough (when the index has one) and every step, each with the step before it.
pub(super) async fn one(Cur(s): Cur, Query(q): Query<OneQ>) -> impl IntoResponse {
    let g = &s.data.graph;
    let Some(i) = g.lookup(&q.id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "no such symbol"})),
        );
    };
    let entry = g.node(i);
    let extras = s.extras().await;
    let walk = extras.flows.get(&entry.id);
    (
        StatusCode::OK,
        Json(json!({
            "entry": entry,
            "narration": walk.map(|(n, _)| n),
            "questions": walk.map(|(_, q)| q),
            "steps": g.flow(i, 8),
        })),
    )
}

#[derive(Deserialize)]
pub(super) struct SourceQ {
    repo: String,
    /// `path:line`, relative to the repository.
    at: String,
    #[serde(default = "around")]
    around: usize,
}

fn around() -> usize {
    6
}

/// The lines around `path:line` in this machine's copy of `repo` (comindly's checkout of its default branch when there is one).
pub(super) async fn source(Conf(cfg): Conf, Query(q): Query<SourceQ>) -> impl IntoResponse {
    let run = || -> anyhow::Result<Value> {
        let (path, line) = q.at.rsplit_once(':').unwrap_or((q.at.as_str(), "1"));
        let line: usize = line.parse().unwrap_or(1).max(1);
        let entry = cfg
            .repos
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
        let lines: Vec<Value> = text
            .lines()
            .enumerate()
            .skip(from)
            .take(q.around * 2 + 1)
            .map(|(n, l)| json!({"n": n + 1, "text": l}))
            .collect();
        Ok(json!({"path": path, "line": line, "lines": lines}))
    };
    match run() {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": super::error_text(&e)})),
        ),
    }
}
