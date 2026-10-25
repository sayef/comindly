//! What the read commands answer, over HTTP: the same [`crate::answers`] a local run computes, so
//! `--server` prints the same thing.

use anyhow::Context as _;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use super::Cur;
use crate::api;

fn answer<T: serde::Serialize>(
    r: anyhow::Result<Option<T>>,
    missing: impl Into<String>,
) -> Response {
    match r {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => super::error_body(StatusCode::NOT_FOUND, missing),
        Err(e) => super::bad(e).into_response(),
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

pub(super) async fn find(Cur(s): Cur, Query(q): Query<api::AnswerQuery>) -> Json<Vec<api::Node>> {
    Json(crate::answers::find(&s.data.graph, &q.q, &repos(&q.repos)))
}

pub(super) async fn search(Cur(s): Cur, Query(q): Query<api::AnswerQuery>) -> Response {
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

pub(super) async fn why(Cur(s): Cur, Query(q): Query<api::AnswerFocus>) -> Response {
    let entries = s.entries().await;
    let w = crate::answers::why(&s.data.graph, &entries, &q.focus);
    answer(Ok(w), format!("no symbol matches `{}`", q.focus))
}

pub(super) async fn ledger(
    Cur(s): Cur,
    Query(q): Query<api::LedgerQuery>,
) -> Json<api::answers::Ledger> {
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
}

pub(super) async fn flow(Cur(s): Cur, Query(q): Query<api::AnswerFocus>) -> Response {
    let extras = s.extras().await;
    let f = crate::answers::flow(&s.data.graph, &q.focus, |id| extras.flows.get(id).cloned());
    answer(Ok(f), format!("no symbol matching `{}`", q.focus))
}

pub(super) async fn contracts(
    Cur(s): Cur,
    Query(q): Query<api::ContractsQuery>,
) -> Json<api::answers::Contracts> {
    Json(crate::answers::contracts(
        &s.data.graph,
        q.kind.as_deref(),
        q.dangling,
    ))
}

pub(super) async fn conventions(
    Cur(s): Cur,
    Query(q): Query<api::RepoQuery>,
) -> Json<api::answers::Conventions> {
    let manual = s.manual.read().await.clone();
    let all = crate::conventions::apply(s.data.conventions.clone(), &manual);
    Json(crate::answers::conventions(all, &manual, q.repo.as_deref()))
}

pub(super) async fn explore(
    Cur(s): Cur,
    Query(q): Query<api::AnswerFocus>,
) -> Json<api::answers::Explore> {
    Json(crate::answers::explore(&s.data.graph, &q.focus))
}

pub(super) async fn stats(
    Cur(s): Cur,
    Query(q): Query<api::StatsQuery>,
) -> Json<api::answers::Stats> {
    let enriched = s.extras().await.enrichment.keys().cloned().collect();
    Json(crate::answers::stats(
        &s.data.symbols,
        &s.data.edges,
        &enriched,
        &repos(&q.repos),
    ))
}

pub(super) async fn packages(
    Cur(s): Cur,
    Query(q): Query<api::PackagesQuery>,
) -> Json<Vec<crate::manifests::InternalDep>> {
    Json(crate::answers::packages(s.data.deps.clone(), q.skew))
}

/// `preflight`: the caller reads its changes from git; the index answers what they touch.
pub(super) async fn preflight(
    Cur(s): Cur,
    Json(q): Json<api::PreflightRequest>,
) -> Json<api::answers::Preflight> {
    let graph = super::graph_uri(&s);
    let (recipes, packages) = match &graph {
        Some(u) => (
            crate::index::read_recipes(u).await.unwrap_or_default(),
            crate::index::read_packages(u).await.unwrap_or_default(),
        ),
        None => Default::default(),
    };
    let entries = s.entries().await.clone();
    Json(crate::answers::preflight(
        &s.data.graph,
        &s.data.symbols,
        &s.data.edges,
        entries,
        &recipes,
        &packages,
        &q.changes,
    ))
}
