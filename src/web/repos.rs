//! Repositories from the UI: the list `comindly link` indexes, scanning a folder, cloning, and an
//! indexing job whose output the page follows.

use std::path::Path;

use anyhow::Context as _;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{Conf, Cur, Shared, Srv};
use std::sync::Arc;

use crate::config::RepoEntry;
use crate::events::{Event, EventSink, Kind, Level};
use crate::worktree;

/// The current or last indexing run.
#[derive(Default)]
pub(super) struct Job {
    id: Option<String>,
    running: bool,
    cancel: Option<tokio_util::sync::CancellationToken>,
    cancelled: bool,
    ok: Option<bool>,
    log: Arc<JobLog>,
    started: Option<String>,
    finished: Option<String>,
}

/// A job's events, kept for the app and passed on to the server's log.
#[derive(Default)]
pub(super) struct JobLog {
    id: String,
    events: std::sync::Mutex<Vec<Event>>,
    out: crate::ui::Terminal,
}

impl EventSink for JobLog {
    fn emit(&self, mut e: Event) {
        e.job_id = Some(self.id.clone());
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        // Progress replaces the previous progress of the same step, so the log stays short.
        if e.kind == Kind::Progress {
            if let Some(last) = events
                .last_mut()
                .filter(|l| l.kind == Kind::Progress && l.message == e.message)
            {
                *last = e.clone();
                drop(events);
                self.out.emit(e);
                return;
            }
        }
        events.push(e.clone());
        drop(events);
        self.out.emit(e);
    }
}

fn fail(e: anyhow::Error) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"ok": false, "message": super::error_text(&e)})),
    )
}

fn absolute(p: &str) -> String {
    let p = crate::config::expand_tilde(p.trim());
    std::fs::canonicalize(&p)
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or(p)
}

pub(super) async fn list(State(app): State<Shared>, Cur(s): Cur) -> Json<Value> {
    let cfg = app.config();
    let indexed: std::collections::HashMap<String, usize> =
        s.data.graph.repos().into_iter().collect();
    let meta = match super::graph_uri(&s) {
        Some(u) => crate::index::read_indexed_repos(&u)
            .await
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let commits: std::collections::HashMap<String, String> = meta
        .iter()
        .map(|r| (r.name.clone(), r.commit.clone()))
        .collect();
    let repos = cfg.repos.clone();
    let symbols = indexed.clone();
    let rows = tokio::task::spawn_blocking(move || {
        repos
            .iter()
            .map(|r| {
                let p = Path::new(&r.path);
                let name = worktree::name(p);
                json!({
                    "path": r.path,
                    "name": name,
                    "exists": p.join(".git").exists(),
                    "branch": worktree::default_branch(p).ok(),
                    "managed": r.remote.is_some(),
                    "cloned": p.exists(),
                    "your_branch": worktree::current_branch(p),
                    "remote": r.remote.clone().or_else(|| worktree::remote_url(p)).map(|u| crate::storage::without_userinfo(&u)),
                    "symbols": symbols.get(&name),
                    "indexed_commit": commits.get(&name),
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    let listed: Vec<String> = rows
        .iter()
        .filter_map(|r| r["name"].as_str().map(str::to_string))
        .collect();
    let mine: Vec<String> = cfg
        .repos
        .iter()
        .filter_map(|r| {
            r.remote
                .clone()
                .or_else(|| worktree::remote_url(Path::new(&r.path)))
        })
        .map(|u| same_repo(&u))
        .collect();
    let team_missing: Vec<String> = app
        .team()
        .repos
        .into_iter()
        .filter(|u| !mine.contains(&same_repo(u)))
        .collect();
    Json(json!({
        "repos": rows,
        "unlisted": unlisted(&indexed, &meta, &listed),
        "team_missing": team_missing,
        "root": s.data.root,
    }))
}

/// Repositories in the index with no folder in your list: name, symbols, commit and remote URL.
fn unlisted(
    indexed: &std::collections::HashMap<String, usize>,
    meta: &[crate::index::IndexedRepo],
    listed: &[String],
) -> Vec<Value> {
    let mut names: Vec<&String> = indexed.keys().collect();
    names.sort();
    names
        .into_iter()
        .filter(|n| !listed.contains(n))
        .map(|n| {
            let m = meta.iter().find(|r| &r.name == n);
            json!({
                "name": n,
                "symbols": indexed.get(n),
                "commit": m.map(|r| &r.commit),
                "remote": m.and_then(|r| r.remote.as_deref()).map(crate::storage::without_userinfo),
            })
        })
        .collect()
}

#[derive(Deserialize)]
pub(super) struct ScanQ {
    dir: String,
    #[serde(default)]
    depth: Option<usize>,
}

/// Git repositories under a folder, marked when already listed.
pub(super) async fn scan(Conf(cfg): Conf, Json(q): Json<ScanQ>) -> impl IntoResponse {
    let dir = absolute(&q.dir);
    if !Path::new(&dir).is_dir() {
        return fail(anyhow::anyhow!("{dir} is not a folder"));
    }
    let listed: Vec<String> = cfg.repos.iter().map(|r| absolute(&r.path)).collect();
    let found = tokio::task::spawn_blocking(move || {
        worktree::scan(Path::new(&dir), q.depth.unwrap_or(2).clamp(1, 4))
            .into_iter()
            .map(|p| {
                let path = p.to_string_lossy().into_owned();
                json!({
                    "name": worktree::name(&p),
                    "listed": listed.contains(&path),
                    "remote": worktree::remote_url(&p).map(|u| crate::storage::without_userinfo(&u)),
                    "path": path,
                })
            })
            .collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default();
    (StatusCode::OK, Json(json!({"ok": true, "found": found})))
}

#[derive(Deserialize)]
pub(super) struct Change {
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

/// Add and remove repositories (folders); removed ones lose comindly's worktree, not your folder.
pub(super) async fn change(State(app): State<Shared>, Json(c): Json<Change>) -> impl IntoResponse {
    let mut list = app.config().repos.clone();
    let run = tokio::task::spawn_blocking(move || {
        for p in &c.add {
            let path = absolute(p);
            anyhow::ensure!(
                Path::new(&path).join(".git").exists(),
                "{path} is not a git repository"
            );
            if !list.iter().any(|r| absolute(&r.path) == path) {
                list.push(RepoEntry { path, remote: None });
            }
        }
        let gone: Vec<String> = c.remove.iter().map(|p| absolute(p)).collect();
        list.retain(|r| {
            let drop = gone.contains(&absolute(&r.path));
            if drop {
                let _ = worktree::remove(&crate::config::cache_dir(), Path::new(&r.path));
            }
            !drop
        });
        crate::config::set_repos(&list)?;
        app.keep_config()?;
        anyhow::Ok(list.len())
    })
    .await;
    match run.map_err(anyhow::Error::from).and_then(|r| r) {
        Ok(n) => (
            StatusCode::OK,
            Json(
                json!({"ok": true, "message": format!("saved; {n} {} in the list", if n == 1 { "repository" } else { "repositories" })}),
            ),
        ),
        Err(e) => fail(e),
    }
}

/// Stop the running indexer and everything it started.
pub(super) async fn cancel(State(app): State<Shared>) -> impl IntoResponse {
    let j = app.job.lock().await;
    match j.cancel.as_ref().filter(|_| j.running) {
        Some(token) => {
            token.cancel();
            (
                StatusCode::OK,
                Json(json!({"ok": true, "message": "cancelling; work already saved stays"})),
            )
        }
        None => (
            StatusCode::OK,
            Json(json!({"ok": true, "message": "nothing is indexing"})),
        ),
    }
}

pub(super) async fn job(State(app): State<Shared>) -> Json<Value> {
    let j = app.job.lock().await;
    let events = j.log.events.lock().map(|e| e.clone()).unwrap_or_default();
    let from = events.len().saturating_sub(400);
    Json(json!({
        "id": j.id,
        "running": j.running,
        "cancelled": j.cancelled,
        "ok": j.ok,
        "events": &events[from..],
        "started": j.started,
        "finished": j.finished,
    }))
}

#[derive(Deserialize)]
pub(super) struct IndexQ {
    #[serde(default)]
    llm: bool,
    /// Index without the repositories that have no folder here, dropping them from the index.
    #[serde(default)]
    drop_unlisted: bool,
}

pub(super) async fn start(State(app): State<Shared>, Json(q): Json<IndexQ>) -> impl IntoResponse {
    match begin(&app, q.llm, q.drop_unlisted, None).await {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({"ok": true, "message": "indexing started"})),
        ),
        Err(e) => fail(e),
    }
}

/// Start `comindly link` over the listed repositories into the loaded index; the page follows
/// `/api/index` and the UI switches to the new index when it succeeds. `why` opens the log.
pub(super) async fn begin(
    app: &Shared,
    llm: bool,
    drop_unlisted: bool,
    why: Option<String>,
) -> anyhow::Result<()> {
    let session = app.current.read().await.clone();
    let root = session.data.root.clone().context("read-only UI")?;
    if !drop_unlisted {
        let indexed = session.data.graph.repos().into_iter().collect();
        let listed: Vec<String> = app
            .config()
            .repos
            .iter()
            .map(|r| worktree::name(Path::new(&r.path)))
            .collect();
        let missing: Vec<String> = unlisted(&indexed, &[], &listed)
            .iter()
            .filter_map(|r| r["name"].as_str().map(str::to_string))
            .collect();
        anyhow::ensure!(
            missing.is_empty(),
            "no folder here for {}, which the index has; add them first, or drop them from the index",
            missing.join(", ")
        );
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let log = Arc::new(JobLog {
        id: id.clone(),
        ..Default::default()
    });
    let cancel = tokio_util::sync::CancellationToken::new();
    {
        let mut j = app.job.lock().await;
        anyhow::ensure!(!j.running, "indexing is already running");
        *j = Job {
            id: Some(id),
            running: true,
            cancel: Some(cancel.clone()),
            log: log.clone(),
            started: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        };
    }
    if let Some(w) = why {
        log.emit(Event::new(Level::Info, Kind::Note, w));
    }
    let cfg = app.config();
    let team = crate::team::load(&root)
        .await
        .map(|t| t.settings)
        .unwrap_or_default();
    let mut settings = crate::context::Settings::resolve(&team, &cfg);
    if !llm {
        settings.enrich = false;
        settings.flows = false;
        settings.guide = false;
    }
    let client = if settings.enrich || settings.flows || settings.guide {
        let key = crate::credentials::llm_key(team.name.as_deref(), app.server.as_deref()).0;
        crate::llm::LlmClient::new(
            settings.llm_base_url.as_deref(),
            &settings.llm_model,
            key.as_deref(),
        )
        .ok()
    } else {
        None
    };
    let scip = cfg.scip();
    let mut ctx = crate::context::IndexContext::new(
        root.clone(),
        settings,
        log.clone(),
        crate::config::cache_dir(),
    )
    .with_llm(client)
    .with_git(crate::config::git_credentials(app.server.clone()));
    ctx.cancel = cancel;
    let app2 = app.clone();
    tokio::spawn(async move {
        let run = async {
            let sources = crate::pipeline::checkout(&ctx, &cfg.repos).await?;
            crate::pipeline::link(&ctx, sources, true, scip).await
        };
        let result = run.await;
        let cancelled = matches!(&result, Err(e) if e.is::<crate::context::Cancelled>());
        let ok = result.is_ok();
        match &result {
            Ok(_) => {}
            Err(_) if cancelled => ctx.report.warn("Cancelled; what was already saved stays."),
            Err(e) => ctx.report.error(crate::storage::explain(&format!("{e:#}"))),
        }
        if ok {
            if let Err(e) = app2.reload(&root).await {
                ctx.report.error(format!("reload failed: {e:#}"));
            }
        }
        let mut j = app2.job.lock().await;
        j.running = false;
        j.cancel = None;
        j.cancelled = cancelled;
        j.ok = Some(ok);
        j.finished = Some(chrono::Utc::now().to_rfc3339());
    });
    Ok(())
}

/// What the automatic re-index last found.
#[derive(Default, Clone, serde::Serialize)]
pub(super) struct Watch {
    checked: Option<String>,
    result: String,
}

/// With `auto_index` on, check every freshness interval whether a default branch moved on the
/// remote (`git ls-remote`, no download) and re-index when one did.
pub(super) async fn watch(app: Shared) {
    loop {
        let cfg = app.config();
        tokio::time::sleep(cfg.fresh().max(std::time::Duration::from_secs(60))).await;
        if cfg.auto_index != Some(true) {
            continue;
        }
        let result = match moved(&app).await {
            Ok(m) if m.is_empty() => "every default branch is indexed".to_string(),
            Ok(m) => {
                let why = format!("Automatic re-index: {} moved", m.join(", "));
                match begin(&app, false, false, Some(why.clone())).await {
                    Ok(()) => why,
                    Err(e) => format!("{why}, not started: {e:#}"),
                }
            }
            Err(e) => format!("check failed: {e:#}"),
        };
        *app.watch.lock().await = Watch {
            checked: Some(chrono::Utc::now().to_rfc3339()),
            result,
        };
    }
}

/// Listed repositories whose remote default branch differs from what the index holds.
async fn moved(app: &Shared) -> anyhow::Result<Vec<String>> {
    let uri = super::graph_uri(&app.current.read().await.clone()).context("no index")?;
    let indexed = crate::index::read_repo_commits(&uri)
        .await
        .unwrap_or_default();
    let repos = app.config().repos.clone();
    let creds = crate::config::git_credentials(app.server.clone());
    tokio::task::spawn_blocking(move || {
        let mut out = Vec::new();
        for r in &repos {
            let p = Path::new(&r.path);
            let name = worktree::name(p);
            if !p.exists() {
                out.push(name);
                continue;
            }
            let (_, head) = worktree::remote_default(p, creds.as_ref())?;
            if indexed
                .get(&name)
                .is_none_or(|c| !head.starts_with(c.as_str()) && !c.starts_with(&head))
            {
                out.push(name);
            }
        }
        anyhow::Ok(out)
    })
    .await?
}

pub(super) async fn watch_state(State(app): State<Shared>) -> Json<Value> {
    Json(json!({
        "enabled": app.config().auto_index == Some(true),
        "last": *app.watch.lock().await,
    }))
}

/// A clone URL reduced to host and path, so HTTPS and SSH forms of one repository compare equal.
fn same_repo(url: &str) -> String {
    crate::hosting::managed_path(url)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| url.to_string())
}

#[derive(Deserialize)]
pub(super) struct RemoteListQ {
    url: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

/// The repositories of a GitHub organisation or GitLab group, with the ones already here marked.
pub(super) async fn remote_list(
    Srv(server): Srv,
    Conf(cfg): Conf,
    who: Option<axum::Extension<crate::server::Who>>,
    Json(q): Json<RemoteListQ>,
) -> impl IntoResponse {
    let run = async {
        let source = crate::hosting::parse(&q.url)?;
        let given = q
            .token
            .filter(|t| !t.trim().is_empty())
            .map(|t| worktree::Auth {
                username: q
                    .username
                    .filter(|u| !u.trim().is_empty())
                    .unwrap_or_else(|| worktree::token_user(&source.host).into()),
                token: t.trim().to_string(),
            });
        let host = source.host.clone();
        let auth = match given.clone() {
            Some(a) => Some(a),
            None => {
                let server = server.clone();
                tokio::task::spawn_blocking(move || {
                    let local = server.is_none();
                    let token = crate::config::git_credentials(server).token(&host);
                    match token {
                        Some(token) => Some(worktree::Auth {
                            username: worktree::token_user(&host).into(),
                            token,
                        }),
                        None if local => worktree::helper_auth(&host),
                        None => None,
                    }
                })
                .await?
            }
        };
        let repos = crate::hosting::list(&source, auth.as_ref()).await?;
        let mut warning = None;
        if let Some(a) = given {
            let host = source.host.clone();
            match (&server, who) {
                (Some(server), Some(w)) => {
                    if let Err(e) = server.store.set_secret(
                        &w.email,
                        &crate::server::git_secret(&host),
                        &a.token,
                    ) {
                        warning = Some(format!("the token worked but was not kept: {e:#}"));
                    }
                }
                _ => tokio::task::spawn_blocking(move || worktree::approve(&host, &a)).await?,
            }
        }
        let here: Vec<String> = cfg
            .repos
            .iter()
            .filter_map(|r| {
                r.remote
                    .clone()
                    .or_else(|| worktree::remote_url(Path::new(&r.path)))
            })
            .map(|u| same_repo(&u))
            .collect();
        let rows: Vec<Value> = repos
            .iter()
            .map(|r| {
                let mut v = json!(r);
                v["listed"] = json!(here.contains(&same_repo(&r.clone_url)));
                v
            })
            .collect();
        anyhow::Ok(json!({
            "ok": true,
            "provider": source.provider,
            "host": source.host,
            "owner": source.owner,
            "repos": rows,
            "warning": warning,
        }))
    };
    match run.await {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => match e.downcast_ref::<worktree::NeedsToken>() {
            Some(n) => (
                StatusCode::UNAUTHORIZED,
                Json(
                    json!({"ok": false, "message": e.to_string(), "needs_token": {"host": n.host, "username": n.username}}),
                ),
            ),
            None => fail(e),
        },
    }
}

#[derive(Deserialize)]
pub(super) struct RemoteAddQ {
    urls: Vec<String>,
    #[serde(default)]
    share: bool,
}

/// Add repositories by clone URL; comindly clones them into its own folder when indexing.
pub(super) async fn remote_add(
    State(app): State<Shared>,
    Cur(s): Cur,
    Json(q): Json<RemoteAddQ>,
) -> impl IntoResponse {
    let run = async {
        let mut list = app.config().repos.clone();
        let mut added = 0;
        for url in &q.urls {
            let path = crate::hosting::managed_path(url)?
                .to_string_lossy()
                .into_owned();
            if !list.iter().any(|r| r.path == path) {
                list.push(RepoEntry {
                    path,
                    remote: Some(url.clone()),
                });
                added += 1;
            }
        }
        crate::config::set_repos(&list)?;
        app.keep_config()?;
        let mut shared = 0;
        if q.share {
            let root = s.data.root.clone().context("no index to share with")?;
            let stored = crate::team::load(&root).await?;
            let mut settings = stored.settings;
            for url in &q.urls {
                if !settings
                    .repos
                    .iter()
                    .any(|u| same_repo(u) == same_repo(url))
                {
                    settings.repos.push(url.clone());
                    shared += 1;
                }
            }
            if shared > 0 {
                crate::team::save(&root, &settings, stored.version).await?;
                app.set_team(settings);
            }
        }
        anyhow::Ok(match (added, q.share) {
            (_, true) => format!(
                "added {added}, {shared} new for the team; press Index now to clone and index them"
            ),
            _ => format!("added {added}; press Index now to clone and index them"),
        })
    };
    match run.await {
        Ok(m) => (StatusCode::OK, Json(json!({"ok": true, "message": m}))),
        Err(e) => fail(e),
    }
}
