//! Flows across repositories, and the source lines behind a `path:line`.

use axum::extract::Query;
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

/// The lines around `path:line`, from the file contents stored in the index.
pub(super) async fn source(
    Cur(s): Cur,
    Query(q): Query<api::SourceQuery>,
) -> Reply<api::SourceLines> {
    let run = async {
        let (path, line) = q.at.rsplit_once(':').unwrap_or((q.at.as_str(), "1"));
        let line: usize = line.parse().unwrap_or(1).max(1);
        let uri = super::graph_uri(&s).ok_or_else(|| anyhow::anyhow!("no index"))?;
        let text = crate::index::read_source(&uri, &q.repo, path)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("{path} is not stored in this index; index again to show its code")
            })?;
        let repo = crate::index::read_indexed_repos(&uri)
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|r| r.name == q.repo);
        let link = repo
            .as_ref()
            .and_then(|r| crate::hosting::file_link(r.remote.as_deref()?, &r.commit, path, line));
        anyhow::Ok(api::SourceLines {
            path: path.to_string(),
            line,
            lines: crate::sources::around(&text, line, q.around)
                .into_iter()
                .map(|(n, text)| api::SourceLine { n, text })
                .collect(),
            commit: repo.map(|r| r.commit),
            link,
        })
    };
    run.await.map(Json).map_err(super::bad)
}
