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

/// The `repos` filter narrowed to the teams' repositories; `None` when the teams label none of them.
fn scope(s: &super::Session, q: &api::AnswerQuery) -> Option<Vec<String>> {
    let repos = repos(&q.repos);
    let selected = crate::team::parse_selection(q.teams.as_deref());
    if selected.is_empty() {
        return Some(repos);
    }
    let teams = s.teams.read().map(|t| t.clone()).unwrap_or_default();
    let scoped: Vec<String> = s
        .data
        .graph
        .repos()
        .into_iter()
        .map(|(r, _)| r)
        .filter(|r| teams.in_view(r, &selected))
        .filter(|r| repos.is_empty() || repos.contains(r))
        .collect();
    (!scoped.is_empty()).then_some(scoped)
}

pub(super) async fn find(Cur(s): Cur, Query(q): Query<api::AnswerQuery>) -> Json<Vec<api::Node>> {
    let Some(repos) = scope(&s, &q) else {
        return Json(Vec::new());
    };
    Json(crate::answers::find(&s.data.graph, &q.q, &repos))
}

pub(super) async fn search(Cur(s): Cur, Query(q): Query<api::AnswerQuery>) -> Response {
    let run = async {
        let tables = s.tables.as_ref().context("no index")?;
        let engine = s
            .search()
            .await
            .context("the embedding model could not be loaded")?;
        let extras = s.extras().await;
        let Some(repos) = scope(&s, &q) else {
            return anyhow::Ok(None);
        };
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
    let discussions = super::discussions(&s).await;
    let w = crate::answers::why(&s.data.graph, &entries, &discussions, &q.focus);
    answer(Ok(w), format!("no symbol matches `{}`", q.focus))
}

pub(super) async fn hook(Cur(s): Cur, Query(q): Query<api::HookQuery>) -> Json<api::answers::Hook> {
    let entries = s.entries().await;
    let seen = repos(&q.seen).into_iter().collect();
    Json(crate::answers::hook(
        &s.data.symbols,
        &entries,
        &q.event,
        &q.cwd,
        q.path.as_deref(),
        &seen,
    ))
}

/// Each repository's style guide, `(repo, markdown)`.
pub(super) async fn guide(
    Cur(s): Cur,
    Query(q): Query<api::StatsQuery>,
) -> Json<Vec<(String, String)>> {
    let want = repos(&q.repos);
    let guides = match super::graph_uri(&s) {
        Some(u) => crate::index::read_style_guide(&u).await.unwrap_or_default(),
        None => Vec::new(),
    };
    Json(
        guides
            .into_iter()
            .filter(|(r, _)| want.is_empty() || want.iter().any(|w| w.eq_ignore_ascii_case(r)))
            .collect(),
    )
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
    let mut answer = crate::answers::preflight(
        &s.data.graph,
        &s.data.symbols,
        &s.data.edges,
        entries,
        &recipes,
        &packages,
        &q.changes,
    );
    let teams = s.teams.read().unwrap_or_else(|e| e.into_inner()).clone();
    answer.markdown +=
        &crate::preflight::teams_affected(&s.data.graph, &s.data.symbols, &q.changes, &teams);
    answer.markdown += &crate::preflight::discussions(
        &s.data.graph,
        &s.data.symbols,
        &super::discussions(&s).await,
        &q.changes,
    );
    Json(answer)
}
