//! What the read commands answer, over HTTP: the same [`crate::answers`] a local run computes, so
//! `--server` prints the same thing.

use anyhow::Context as _;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use super::Cur;

fn answer<T: serde::Serialize>(
    r: anyhow::Result<Option<T>>,
    missing: impl Into<String>,
) -> Response {
    match r {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => super::error_body(StatusCode::NOT_FOUND, missing).into_response(),
        Err(e) => super::error_body(StatusCode::BAD_REQUEST, super::error_text(&e)).into_response(),
    }
}

/// Repositories as a comma-separated list.
fn repos(r: &Option<String>) -> Vec<String> {
    r.iter()
        .flat_map(|r| r.split(','))
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(String::from)
        .collect()
}

#[derive(Deserialize)]
pub(super) struct QueryQ {
    q: String,
    repos: Option<String>,
}

pub(super) async fn find(Cur(s): Cur, Query(q): Query<QueryQ>) -> Response {
    Json(crate::answers::find(&s.data.graph, &q.q, &repos(&q.repos))).into_response()
}

pub(super) async fn search(Cur(s): Cur, Query(q): Query<QueryQ>) -> Response {
    let run = async {
        let tables = s.tables.as_ref().context("no index")?;
        let engine = s
            .search()
            .await
            .context("the embedding model could not be loaded")?;
        let extras = s.extras().await;
        let repos = repos(&q.repos);
        let hits = crate::search::hybrid(
            tables,
            &engine.by_id,
            &s.data.graph,
            &extras.enrichment,
            &engine.embedder,
            &q.q,
            crate::answers::search_limit(&repos),
        )
        .await
        .context("hybrid search failed (was the index built with --no-embed?)")?;
        anyhow::Ok(Some(crate::answers::search(&q.q, hits, &repos)))
    };
    answer(run.await, "")
}

#[derive(Deserialize)]
pub(super) struct FocusQ {
    focus: String,
}

pub(super) async fn why(Cur(s): Cur, Query(q): Query<FocusQ>) -> Response {
    let entries = s.entries().await;
    let w = crate::answers::why(&s.data.graph, &entries, &q.focus);
    answer(Ok(w), format!("no symbol matches `{}`", q.focus))
}

#[derive(Deserialize)]
pub(super) struct LedgerQ {
    #[serde(default)]
    inbox: bool,
    #[serde(default)]
    check: bool,
}

pub(super) async fn ledger(Cur(s): Cur, Query(q): Query<LedgerQ>) -> Response {
    let packages = match super::graph_uri(&s) {
        Some(u) => crate::index::read_packages(&u).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let entries = s.entries().await.clone();
    Json(crate::answers::ledger(
        entries,
        &s.data.symbols,
        &s.data.edges,
        &packages,
        q.inbox,
        q.check,
    ))
    .into_response()
}

pub(super) async fn flow(Cur(s): Cur, Query(q): Query<FocusQ>) -> Response {
    let extras = s.extras().await;
    let f = crate::answers::flow(&s.data.graph, &q.focus, |id| extras.flows.get(id).cloned());
    answer(Ok(f), format!("no symbol matching `{}`", q.focus))
}

#[derive(Deserialize)]
pub(super) struct ContractsQ {
    kind: Option<String>,
    #[serde(default)]
    dangling: bool,
}

pub(super) async fn contracts(Cur(s): Cur, Query(q): Query<ContractsQ>) -> Response {
    Json(crate::answers::contracts(
        &s.data.graph,
        q.kind.as_deref(),
        q.dangling,
    ))
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct RepoQ {
    repo: Option<String>,
}

pub(super) async fn conventions(Cur(s): Cur, Query(q): Query<RepoQ>) -> Response {
    let manual = s.manual.read().await.clone();
    let all = crate::conventions::apply(s.data.conventions.clone(), &manual);
    Json(crate::answers::conventions(all, &manual, q.repo.as_deref())).into_response()
}
