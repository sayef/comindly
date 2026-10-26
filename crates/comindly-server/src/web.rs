//! Web UI (`comindly ui --web`): one embedded page over a small JSON API. Locally it answers on
//! localhost to the link's token; in server mode members sign in. Ledger writes go to the index root.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use tokio::sync::{OnceCell, RwLock};

use crate::api;
use crate::graph::{Node, Zoom};
use crate::ledger::{Entry, Provenance, Record, RelationKind, Status};
use crate::view::Data;

mod answers;
mod auth;
mod flows;
mod jobs;
mod repos;
mod settings;
mod team;

/// Everything tied to one loaded index; replaced as a whole when the index changes.
struct Session {
    data: Data,
    entries: RwLock<Vec<Entry>>,
    /// When `entries` was last read from the ledger.
    entries_read: tokio::sync::Mutex<std::time::Instant>,
    manual: RwLock<Vec<crate::conventions::Manual>>,
    /// Loaded on first use: LLM summaries and flows, and the search model.
    extras: OnceCell<Extras>,
    search: OnceCell<Option<Search>>,
    /// The index's tables, held open so LanceDB caches their metadata and indexes.
    tables: Option<crate::index::Tables>,
    /// Org maps of earlier index versions, by position in `/api/versions`.
    past: tokio::sync::Mutex<HashMap<usize, api::OrgMap>>,
    /// Where flows start, ranked once per loaded index.
    flow_list: OnceCell<Vec<api::FlowRow>>,
}

#[derive(Default)]
struct Extras {
    enrichment: crate::search::Enrichment,
    flows: HashMap<String, (String, Vec<String>)>,
}

struct Search {
    embedder: Arc<crate::embed::Embedder>,
    by_id: HashMap<String, crate::model::Symbol>,
}

impl Session {
    async fn extras(&self) -> &Extras {
        self.extras
            .get_or_init(|| async {
                let Some(uri) = graph_uri(self) else {
                    return Extras::default();
                };
                let enrichment = crate::index::read_enrichment(&uri).await.ok().flatten();
                let flows = crate::index::read_flows(&uri).await.ok().flatten();
                Extras {
                    enrichment: enrichment
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(id, s, q)| (id.render(), (s, q)))
                        .collect(),
                    flows: flows
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(id, n, q)| (id.render(), (n, q)))
                        .collect(),
                }
            })
            .await
    }

    async fn search(&self) -> Option<&Search> {
        self.search
            .get_or_init(|| async {
                let model = match &self.data.root {
                    Some(root) => crate::config::index_settings(root).await.embed_model,
                    None => crate::embed::DEFAULT_MODEL.to_string(),
                };
                let embedder =
                    tokio::task::spawn_blocking(move || crate::embed::Embedder::shared(&model))
                        .await
                        .ok()?
                        .ok()?;
                let by_id = self
                    .data
                    .symbols
                    .iter()
                    .map(|s| (s.id.render(), s.clone()))
                    .collect();
                Some(Search { embedder, by_id })
            })
            .await
            .as_ref()
    }
}

struct AppState {
    current: RwLock<Arc<Session>>,
    jobs: crate::jobs::Jobs,
    watch: tokio::sync::Mutex<api::WatchResult>,
    /// Members, sign-in and stored secrets, in server mode.
    server: Option<Arc<crate::server::Server>>,
    /// The loaded index's shared settings.
    team: std::sync::RwLock<crate::team::TeamSettings>,
    /// `config.toml` as last read, with the file's modification time.
    /// Your preferences as last read, and when; `None` reads them again.
    config: std::sync::Mutex<(Arc<crate::config::Config>, Option<std::time::Instant>)>,
    /// What agents reach at `/mcp` on a server.
    mcp: Option<crate::mcp::Shared>,
    mcp_generation: Arc<std::sync::atomic::AtomicU64>,
}

type Shared = Arc<AppState>;

/// The session a request works on: the index loaded when it arrived.
struct Cur(Arc<Session>);

impl axum::extract::FromRequestParts<Shared> for Cur {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _: &mut axum::http::request::Parts,
        app: &Shared,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(Cur(app.current.read().await.clone()))
    }
}

/// The server, in server mode.
struct Srv(Option<Arc<crate::server::Server>>);

impl axum::extract::FromRequestParts<Shared> for Srv {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        _: &mut axum::http::request::Parts,
        app: &Shared,
    ) -> std::result::Result<Self, Self::Rejection> {
        Ok(Srv(app.server.clone()))
    }
}

impl Session {
    /// Ledger entries, re-read when a few seconds old: agents and colleagues write records too.
    async fn entries(&self) -> tokio::sync::RwLockReadGuard<'_, Vec<Entry>> {
        // One request refreshes; the others read what is there without waiting on storage.
        let due = {
            let mut read = self.entries_read.lock().await;
            let due = read.elapsed() > std::time::Duration::from_secs(15);
            if due {
                *read = std::time::Instant::now();
            }
            due
        };
        if let (true, Some(root)) = (due, self.data.root.clone()) {
            let opened =
                tokio::task::spawn_blocking(move || crate::ledger::Ledger::open(&root)).await;
            if let Ok(Ok(ledger)) = opened {
                if let Ok(records) = ledger.all().await {
                    *self.entries.write().await = crate::ledger::derive(
                        &records,
                        &self.data.symbols,
                        &crate::config::local_verifier(),
                    );
                }
            }
        }
        self.entries.read().await
    }

    async fn open(mut data: Data, fresh: std::time::Duration) -> Session {
        let entries = std::mem::take(&mut data.entries);
        let manual = std::mem::take(&mut data.manual);
        let tables = match data.root.as_ref() {
            Some(r) => {
                let uri = format!("{}/_graph", r.trim_end_matches('/'));
                crate::index::Tables::connect(&uri, fresh).await.ok()
            }
            None => None,
        };
        Session {
            data,
            entries: RwLock::new(entries),
            entries_read: tokio::sync::Mutex::new(std::time::Instant::now()),
            manual: RwLock::new(manual),
            extras: OnceCell::new(),
            search: OnceCell::new(),
            tables,
            past: Default::default(),
            flow_list: OnceCell::new(),
        }
    }
}

impl AppState {
    /// `config.toml`, read again only when the file changed.
    /// Your preferences, read again every few seconds: the CLI may change them too.
    fn config(&self) -> Arc<crate::config::Config> {
        let mut c = self.config.lock().unwrap_or_else(|e| e.into_inner());
        if c.1
            .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(5))
        {
            *c = (
                Arc::new(crate::config::Config::load()),
                Some(std::time::Instant::now()),
            );
        }
        c.0.clone()
    }

    /// After a change here: read the preferences again on next use.
    fn config_changed(&self) {
        self.config.lock().unwrap_or_else(|e| e.into_inner()).1 = None;
    }

    fn team(&self) -> crate::team::TeamSettings {
        self.team.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_team(&self, team: crate::team::TeamSettings) {
        *self.team.write().unwrap_or_else(|e| e.into_inner()) = team;
    }

    /// The settings the loaded index runs with here.
    fn settings(&self) -> crate::context::Settings {
        crate::context::Settings::resolve(&self.team(), crate::config::env)
    }

    /// The repositories this app indexes: the server's, else this machine's in `config.toml`.
    fn repos(&self) -> Vec<crate::config::RepoEntry> {
        match &self.server {
            Some(s) => s.settings.repos(),
            None => self.config().repos.clone(),
        }
    }

    fn set_repos(&self, by: Option<&str>, list: &[crate::config::RepoEntry]) -> Result<()> {
        match &self.server {
            Some(s) => s.settings.set_repos(by.unwrap_or("admin"), list),
            None => crate::config::set_repos(list).map(|()| self.config_changed()),
        }
    }

    /// Load the index at `root` and make it the one every request sees.
    async fn reload(&self, root: &str) -> Result<()> {
        let owned = root.to_string();
        let data = tokio::task::spawn_blocking(move || {
            Data::load_or_empty(&owned, &crate::config::local_verifier())
        })
        .await?;
        let fresh = self.config().fresh();
        let team = crate::team::load(root)
            .await
            .map(|t| t.settings)
            .unwrap_or_default();
        self.set_team(team);
        *self.current.write().await = Arc::new(Session::open(data, fresh).await);
        self.reload_mcp(root);
        Ok(())
    }

    /// Load the index at `root` for agents in the background, keeping the previous one until then.
    fn reload_mcp(&self, root: &str) {
        let Some(mcp) = self.mcp.clone() else {
            return;
        };
        let uri = format!("{}/_graph", root.trim_end_matches('/'));
        let generation = self
            .mcp_generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        let latest = self.mcp_generation.clone();
        let (fresh, markdown) = (self.config().fresh(), self.settings().format != "json");
        tokio::spawn(async move {
            match crate::mcp::load(&uri, markdown, fresh).await {
                // A slower, older load never replaces a newer one.
                Ok(server) if latest.load(std::sync::atomic::Ordering::SeqCst) == generation => {
                    if let Ok(mut m) = mcp.write() {
                        *m = Some(server);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    crate::ui::warn(&format!("agents cannot use {uri} yet: {}", error_text(&e)))
                }
            }
        });
    }
}

/// Who the UI lets in: whoever holds the local link's token, or signed-in members of a server.
pub enum Access {
    Local(String),
    Server(Arc<crate::server::Server>),
}

/// Serve the UI on `listener` until the process ends.
pub fn serve(data: Data, listener: std::net::TcpListener, access: Access) -> Result<()> {
    let rt = crate::rt::shared();
    let root = data.root.clone();
    let server = match &access {
        Access::Server(s) => Some(s.clone()),
        Access::Local(_) => None,
    };
    let cfg = Arc::new(crate::config::Config::load());
    let team = match &root {
        Some(r) => rt
            .block_on(crate::team::load(r))
            .map(|t| t.settings)
            .unwrap_or_default(),
        None => Default::default(),
    };
    let state = Arc::new(AppState {
        team: std::sync::RwLock::new(team),
        current: RwLock::new(Arc::new(rt.block_on(Session::open(data, cfg.fresh())))),
        config: std::sync::Mutex::new((cfg, Some(std::time::Instant::now()))),
        jobs: crate::jobs::Jobs::new(
            match &server {
                Some(s) => s.jobs.clone(),
                None => Arc::new(crate::jobs::MemoryJobs::default()),
            },
            Arc::new(crate::ui::Terminal::new()),
        ),
        watch: Default::default(),
        mcp: server.as_ref().map(|_| Default::default()),
        server,
        mcp_generation: Default::default(),
    });
    if let Some(root) = root {
        let _guard = rt.enter();
        state.reload_mcp(&root);
    }
    rt.spawn(repos::watch(state.clone()));
    if let Some(s) = state.server.clone() {
        rt.spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
                let s = s.clone();
                let _ = tokio::task::spawn_blocking(move || s.accounts.tidy()).await;
            }
        });
    }
    let app = Router::new()
        .route("/api/v1/overview", get(overview))
        .route("/api/v1/orgmap", get(orgmap))
        .route("/api/v1/versions", get(versions))
        .route("/api/v1/contracts", get(contracts))
        .route("/api/v1/ledger", get(ledger))
        .route(
            "/api/v1/view",
            get(view).layer(tower_http::compression::CompressionLayer::new()),
        )
        .route(
            "/api/v1/conventions",
            get(conventions).post(change_convention),
        )
        .route("/api/v1/find", get(find))
        .route("/api/v1/node", get(node))
        .route("/api/v1/search", get(search))
        .route("/api/v1/context", get(context))
        .route("/api/v1/packages", get(packages))
        .route("/api/v1/flows", get(flows::list))
        .route("/api/v1/flow", get(flows::one))
        .route("/api/v1/source", get(flows::source))
        .route("/api/v1/setup", get(setup))
        .route("/api/v1/review", post(review))
        .route("/api/v1/erase", post(erase))
        .route("/api/v1/remember", post(remember))
        .route("/api/v1/settings", get(settings::get).post(settings::save))
        .route("/api/v1/settings/test", post(settings::test))
        .route("/api/v1/repos", get(repos::list).post(repos::change))
        .route("/api/v1/repos/scan", post(repos::scan))
        .route("/api/v1/repos/remote/list", post(repos::remote_list))
        .route("/api/v1/repos/remote/add", post(repos::remote_add))
        .route("/api/v1/index", get(repos::job).post(repos::start))
        .route("/api/v1/index/cancel", post(repos::cancel))
        .route("/api/v1/index/watch", get(repos::watch_state))
        .route("/api/v1/answer/find", get(answers::find))
        .route("/api/v1/answer/search", get(answers::search))
        .route("/api/v1/answer/why", get(answers::why))
        .route("/api/v1/answer/ledger", get(answers::ledger))
        .route("/api/v1/answer/flow", get(answers::flow))
        .route("/api/v1/answer/contracts", get(answers::contracts))
        .route("/api/v1/answer/explore", get(answers::explore))
        .route("/api/v1/answer/stats", get(answers::stats))
        .route("/api/v1/answer/packages", get(answers::packages))
        .route("/api/v1/answer/preflight", post(answers::preflight))
        .route("/api/v1/answer/conventions", get(answers::conventions))
        .route("/api/v1/jobs", get(jobs::list))
        .route("/api/v1/jobs/{id}", get(jobs::one))
        .route("/api/v1/jobs/{id}/events", get(jobs::events))
        .route("/api/v1/jobs/{id}/cancel", post(jobs::cancel))
        .route("/api/v1/team", get(team::get).post(team::save))
        .route("/api/v1/team/key", post(team::key))
        .route("/api/v1/join", post(team::join))
        .route("/api/v1/team/create", post(team::create))
        .route("/api/v1/auth/me", get(auth::me))
        .route("/health", get(health))
        .route("/ready", get(ready))
        .fallback(get(asset));
    let local = matches!(access, Access::Local(_));
    let app = match access {
        Access::Local(token) => app
            .layer(axum::middleware::from_fn_with_state(Arc::new(token), guard))
            .layer(axum::middleware::from_fn(local_only)),
        Access::Server(server) => app
            .route("/api/v1/auth/link", post(auth::request_link))
            .route("/api/v1/auth/redeem", post(auth::redeem))
            .route("/api/v1/auth/code", post(auth::sign_in_code))
            .route("/api/v1/auth/reset-code", post(auth::reset_own_code))
            .route("/api/v1/auth/signout", post(auth::sign_out))
            .route(auth::SELF_REVOKE, post(auth::revoke_self))
            .route("/api/v1/auth/device/start", post(auth::device_start))
            .route("/api/v1/auth/device/token", post(auth::device_token))
            .route("/api/v1/auth/device", get(auth::device_info))
            .route("/api/v1/auth/device/approve", post(auth::device_approve))
            .route("/api/v1/members", get(auth::members))
            .route("/api/v1/members/invite", post(auth::invite))
            .route("/api/v1/members/reset", post(auth::reset_code))
            .route("/api/v1/members/role", post(auth::set_role))
            .route("/api/v1/members/remove", post(auth::remove))
            .route("/api/v1/members/revoke", post(auth::revoke_invite))
            .route("/api/v1/tokens", get(auth::tokens).post(auth::create_token))
            .route("/api/v1/tokens/revoke", post(auth::revoke_token))
            .route("/api/v1/secrets", get(auth::secrets).post(auth::set_secret))
            .route("/api/v1/audit", get(auth::audit))
            .merge({
                let shared = state.mcp.clone().unwrap_or_default();
                Router::new()
                    .nest_service(
                        "/mcp",
                        crate::mcp::http(shared.clone(), auth::hosts(&server.config)),
                    )
                    .layer(axum::middleware::from_fn_with_state(shared, mcp_ready))
            })
            .layer(axum::middleware::from_fn_with_state(server, auth::guard)),
    }
    .with_state(state.clone())
    .layer(axum::middleware::from_fn_with_state(local, traced));
    let stopping = state;
    listener.set_nonblocking(true)?;
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)?;
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown(stopping))
        .await
        .context("serve")
    })
}

/// On Ctrl-C or SIGTERM: stop taking requests and let a running job stop at its next checkpoint.
async fn shutdown(app: Shared) {
    let term = async {
        #[cfg(unix)]
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
            return;
        }
        std::future::pending::<()>().await
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term => {}
    }
    crate::ui::note("stopping");
    if let Some(job) = app.jobs.shutdown() {
        job.cancel.cancel();
        let _ = tokio::time::timeout(std::time::Duration::from_secs(20), job.wait()).await;
    }
    // Open event streams and agent sessions do not hold the process.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        crate::telemetry::flush();
        std::process::exit(0);
    });
}

/// The id that ties a request's log lines, the job it starts and its trace together.
#[derive(Clone)]
pub(crate) struct RequestId(pub String);

/// A caller's `X-Request-Id` is kept when it is a short token of letters, digits, `-`, `_`, `.`.
fn safe_request_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// Gives each request an id and a span, and logs one access line for it: method, path without
/// the query, status, time and the member's email. Locally, and for health checks, at debug.
async fn traced(
    State(local): State<bool>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use tracing::Instrument;
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| safe_request_id(v))
        .map(str::to_string)
        .unwrap_or_else(new_token);
    req.extensions_mut().insert(RequestId(id.clone()));
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let route = req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .map_or("/*", |m| m.as_str())
        .to_string();
    let span = tracing::info_span!(
        "request",
        otel.name = %format!("{method} {route}"),
        otel.kind = "server",
        otel.status_code = tracing::field::Empty,
        request_id = %id,
        http.request.method = %method,
        http.route = %route,
        url.path = %path,
        http.response.status_code = tracing::field::Empty,
    );
    let start = std::time::Instant::now();
    let mut res = next.run(req).instrument(span.clone()).await;
    let status = res.status().as_u16();
    span.record("http.response.status_code", status);
    if status >= 500 {
        span.record("otel.status_code", "ERROR");
    }
    let ms = start.elapsed().as_millis() as u64;
    let email = res
        .extensions()
        .get::<crate::server::Who>()
        .map(|w| w.email.clone());
    span.in_scope(|| {
        let quiet = local || path == "/health" || path == "/ready";
        macro_rules! access {
            ($l:expr) => {
                tracing::event!(
                    $l,
                    kind = "access",
                    method = %method,
                    path = %path,
                    status,
                    duration_ms = ms,
                    email = email.as_deref(),
                    "{method} {path} {status} {ms} ms"
                )
            };
        }
        if quiet {
            access!(tracing::Level::DEBUG)
        } else {
            access!(tracing::Level::INFO)
        }
    });
    if let Ok(v) = header::HeaderValue::from_str(&id) {
        res.headers_mut().insert("x-request-id", v);
    }
    res
}

/// The one error shape, `{"error": {"code", "message"}}`, with a status that means what it says.
pub(crate) fn error_body(status: StatusCode, message: impl Into<String>) -> Response {
    failure(status, message).into_response()
}

/// A failed request, answered as [`api::ErrorBody`].
pub(crate) struct Failure(StatusCode, api::ErrorDetail);

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, Json(api::ErrorBody { error: self.1 })).into_response()
    }
}

pub(crate) fn failure(status: StatusCode, message: impl Into<String>) -> Failure {
    let code = match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::CONFLICT => "conflict",
        StatusCode::GONE => "gone",
        StatusCode::SERVICE_UNAVAILABLE => "unavailable",
        _ => "error",
    };
    let error = api::ErrorDetail {
        code: code.into(),
        message: message.into(),
        host: None,
        username: None,
    };
    Failure(status, error)
}

/// A handler's answer: the typed body, or the error shape.
pub(crate) type Reply<T> = std::result::Result<Json<T>, Failure>;

/// A failed request's 400 with the error explained.
pub(crate) fn bad(e: anyhow::Error) -> Failure {
    failure(StatusCode::BAD_REQUEST, error_text(&e))
}

/// For load balancers and orchestrators: answers while the process serves requests.
async fn health() -> Json<api::Health> {
    Json(api::Health {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

/// For orchestrators: ready once an index is loaded and, on a server, the stores answer.
async fn ready(State(app): State<Shared>) -> Response {
    let loaded = app.current.read().await.data.root.is_some();
    let stores = match app.server.clone() {
        Some(s) => tokio::task::spawn_blocking(move || s.accounts.members().is_ok())
            .await
            .unwrap_or(false),
        None => true,
    };
    let status = if loaded && stores {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let ready = api::Ready {
        ready: status == StatusCode::OK,
        index: loaded,
        stores,
    };
    (status, Json(ready)).into_response()
}

/// Agents get 503 with a reason until there is an index to answer from.
async fn mcp_ready(
    State(shared): State<crate::mcp::Shared>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if shared.read().map(|s| s.is_none()).unwrap_or(true) {
        let mut res = error_body(
            StatusCode::SERVICE_UNAVAILABLE,
            "no index yet: an admin indexes repositories first",
        );
        res.headers_mut()
            .insert(header::RETRY_AFTER, header::HeaderValue::from_static("60"));
        return res;
    }
    next.run(req).await
}

/// A random token for one UI session.
pub fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// `?token=` on a page sets the session cookie and redirects to the clean URL; `/api/` requires it.
async fn guard(
    State(token): State<Arc<String>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let query = req.uri().query().unwrap_or("");
    if query.split('&').any(|kv| kv == format!("token={token}")) {
        let rest: Vec<&str> = query
            .split('&')
            .filter(|kv| !kv.starts_with("token="))
            .collect();
        let path = req.uri().path();
        let to = if rest.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{}", rest.join("&"))
        };
        let cookie =
            format!("comindly_token={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=31536000");
        return (
            StatusCode::SEE_OTHER,
            [(header::LOCATION, to), (header::SET_COOKIE, cookie)],
        )
            .into_response();
    }
    // `/api/v1/auth/me` only says "local", so the page can always tell where it is.
    if req.uri().path().starts_with("/api/") && req.uri().path() != "/api/v1/auth/me" {
        let want = format!("comindly_token={token}");
        let ok = req
            .headers()
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .any(|c| c.trim() == want);
        if !ok {
            let msg =
                "open comindly with the link `comindly ui --web` printed (it carries the access token)";
            return error_body(StatusCode::FORBIDDEN, msg).into_response();
        }
    }
    next.run(req).await
}

/// Reject requests whose `Host` is not loopback, so a DNS-rebinding page cannot read the API.
async fn local_only(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let name = host.rsplit_once(':').map_or(host, |(h, _)| h);
    if matches!(name, "127.0.0.1" | "localhost" | "[::1]") {
        next.run(req).await
    } else {
        (
            StatusCode::FORBIDDEN,
            "comindly web UI answers on localhost only",
        )
            .into_response()
    }
}

/// The embedded UI: a file by path, or `index.html` for client-side routes.
async fn asset(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let find = |p: &str| {
        comindly_ui::ASSETS
            .iter()
            .find(|(n, _)| *n == p)
            .map(|(_, b)| *b)
    };
    let (name, body) = match find(path) {
        Some(b) if !path.is_empty() => (path, b),
        _ if path.starts_with("api/") => {
            return error_body(StatusCode::NOT_FOUND, format!("no such endpoint: /{path}"))
                .into_response();
        }
        _ if path.starts_with("assets/") => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain")],
                &b"not found"[..],
            )
                .into_response();
        }
        _ => ("index.html", find("index.html").unwrap_or_default()),
    };
    let mime = match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("json") => "application/json",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    // Bundled assets carry a content hash in their name; the page itself must stay fresh.
    let cache = if name.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, cache)],
        body,
    )
        .into_response()
}

async fn overview(who: Member, Cur(s): Cur) -> Json<api::Overview> {
    let d = &s.data;
    let entries = s.entries().await;
    let mut status: BTreeMap<String, usize> = BTreeMap::new();
    for e in entries.iter() {
        *status
            .entry(format!("{:?}", e.status).to_lowercase())
            .or_default() += 1;
    }
    let linked = d
        .contracts
        .iter()
        .filter(|c| !c.providers.is_empty() && !c.consumers.is_empty())
        .count();
    let cross = d.edges.iter().filter(|e| e.cross_repo).count();
    let skew = d
        .deps
        .iter()
        .filter(|x| x.skewed())
        .map(|x| api::Skew {
            package: x.name.clone(),
            publisher: x.publisher.clone(),
            published: x.published.clone(),
            consumer: x.consumer.clone(),
            locked: x.locked.clone(),
        })
        .collect();
    Json(api::Overview {
        repos: d
            .graph
            .repos()
            .into_iter()
            .map(|(repo, symbols)| api::RepoSymbols { repo, symbols })
            .collect(),
        symbols: d.symbols.len(),
        edges: d.edges.len(),
        cross_repo_edges: cross,
        contracts: d.contracts.len(),
        linked_contracts: linked,
        ledger: status,
        skew,
        // Viewers read only.
        writable: d.root.is_some()
            && who
                .as_ref()
                .is_none_or(|w| w.role.at_least(crate::server::Role::Member)),
        root: d.root.clone(),
        problem: d.problem.clone(),
    })
}

/// An error for the UI: short, with S3 replies explained and credentials removed.
pub(crate) fn error_text(e: &anyhow::Error) -> String {
    crate::storage::explain(&format!("{e:#}"))
}

fn graph_uri(s: &Session) -> Option<String> {
    s.data
        .root
        .as_ref()
        .map(|r| format!("{}/_graph", r.trim_end_matches('/')))
}

/// When each index version was written, oldest first.
async fn versions(Cur(s): Cur) -> Json<Vec<String>> {
    let list = match &s.tables {
        Some(t) => t.versions().await.unwrap_or_default(),
        None => Vec::new(),
    };
    Json(list.into_iter().map(|(_, _, t)| t).collect())
}

async fn orgmap(Cur(s): Cur, Query(q): Query<api::AtQuery>) -> Reply<api::OrgMap> {
    let d = &s.data;
    let Some(i) = q.at else {
        return Ok(Json(org(&d.graph, &d.symbols, &d.edges, &d.contracts)));
    };
    let past = async {
        if let Some(v) = s.past.lock().await.get(&i) {
            return anyhow::Ok(v.clone());
        }
        let tables = s
            .tables
            .as_ref()
            .context("read-only UI has no index root")?;
        let list = tables.versions().await?;
        let (sv, ev, _) = *list.get(i).context("no such version")?;
        let (symbols, edges) = tables.graph_at((sv, ev)).await?;
        let graph = crate::graph::CodeGraph::build(&symbols, &edges);
        let contracts = graph.contracts(None, false);
        let v = org(&graph, &symbols, &edges, &contracts);
        s.past.lock().await.insert(i, v.clone());
        anyhow::Ok(v)
    };
    past.await.map(Json).map_err(bad)
}

/// Repos as nodes; edges aggregate cross-repo code dependencies and contract links.
fn org(
    graph: &crate::graph::CodeGraph,
    symbols: &[crate::model::Symbol],
    edges: &[crate::model::Edge],
    contracts: &[crate::graph::ContractView],
) -> api::OrgMap {
    let repo_of: std::collections::HashMap<String, &str> = symbols
        .iter()
        .map(|x| (x.id.render(), x.repo.0.as_str()))
        .collect();
    // Per (user, provider, kind): how many links, and the contracts they go through.
    let mut links: BTreeMap<(String, String, &str), (usize, std::collections::BTreeSet<String>)> =
        BTreeMap::new();
    for e in edges.iter().filter(|e| e.cross_repo) {
        if let (Some(a), Some(b)) = (repo_of.get(&e.src.render()), repo_of.get(&e.dst.render())) {
            if *a != crate::resolve::CONTRACTS && *b != crate::resolve::CONTRACTS && a != b {
                links
                    .entry((a.to_string(), b.to_string(), "code"))
                    .or_default()
                    .0 += 1;
            }
        }
    }
    let real = |n: &&Node| n.repo != crate::resolve::CONTRACTS;
    // A queue subscribed to a topic is a contract consuming a contract: its own consumers are
    // the topic's consumers too.
    let by_id: std::collections::HashMap<&str, &crate::graph::ContractView> = contracts
        .iter()
        .map(|c| (c.contract.id.as_str(), c))
        .collect();
    for c in contracts {
        let mut consumers: Vec<&Node> = c.consumers.iter().filter(real).collect();
        for via in c.consumers.iter().filter(|n| !real(n)) {
            if let Some(q) = by_id.get(via.id.as_str()) {
                consumers.extend(q.consumers.iter().filter(real));
            }
        }
        for u in consumers {
            for p in c.providers.iter().filter(real).filter(|p| p.repo != u.repo) {
                let link = links
                    .entry((u.repo.clone(), p.repo.clone(), "contract"))
                    .or_default();
                link.0 += 1;
                link.1.insert(c.contract.name.clone());
            }
        }
    }
    let nodes = graph
        .repos()
        .into_iter()
        .map(|(r, n)| api::OrgNode {
            id: r.clone(),
            name: format!("{r} ({n})"),
            repo: r,
            location: String::new(),
        })
        .collect();
    let edges = links
        .into_iter()
        .map(|((src, dst, kind), (weight, via))| api::OrgEdge {
            src,
            dst,
            kind: kind.into(),
            weight,
            via: via.into_iter().collect(),
        })
        .collect();
    api::OrgMap { nodes, edges }
}

async fn contracts(Cur(s): Cur) -> Json<Vec<api::ContractView>> {
    Json(s.data.contracts.clone())
}

async fn ledger(Cur(s): Cur) -> Json<Vec<Entry>> {
    let entries = s.entries().await;
    let mut v: Vec<&Entry> = entries.iter().collect();
    v.sort_by(|a, b| b.record.created.cmp(&a.record.created));
    Json(named(&s, &v))
}

/// What the terminal UI shows, for `comindly ui` against this server.
/// The whole view a client rebuilds its data from, tagged with its hash: a client that already
/// holds that tag gets `304 Not Modified` instead of the graph again.
async fn view(Cur(s): Cur, headers: axum::http::HeaderMap) -> Response {
    let packages = match graph_uri(&s) {
        Some(u) => crate::index::read_packages(&u).await.unwrap_or_default(),
        None => Vec::new(),
    };
    let mut entries = s.entries().await.clone();
    let findings = crate::checks::run(&entries, &s.data.symbols, &s.data.edges, &packages);
    crate::checks::promote(&mut entries, &findings);
    let manual = s.manual.read().await.clone();
    let body = match serde_json::to_vec(&api::View {
        symbols: (&s.data.symbols[..]).into(),
        edges: (&s.data.edges[..]).into(),
        entries: entries.into(),
        conventions: (&s.data.conventions[..]).into(),
        manual: manual.into(),
        deps: (&s.data.deps[..]).into(),
        problem: s.data.problem.clone(),
    }) {
        Ok(b) => b,
        Err(e) => return failure(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let tag = format!("\"{}\"", &blake3::hash(&body).to_hex()[..32]);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == tag))
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, tag)]).into_response();
    }
    (
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (header::ETAG, tag),
        ],
        body,
    )
        .into_response()
}

/// Entries with each anchor named as the graph shows it (`Class.method`).
fn named(s: &Session, entries: &[&Entry]) -> Vec<Entry> {
    let mut out: Vec<Entry> = entries.iter().map(|e| (*e).clone()).collect();
    for a in out.iter_mut().flat_map(|e| e.record.anchors.iter_mut()) {
        if let Some(i) = s.data.graph.lookup(&a.symbol) {
            a.name = s.data.graph.node(i).name;
        }
    }
    out
}

async fn conventions(Cur(s): Cur, Query(q): Query<api::RepoQuery>) -> Json<api::Conventions> {
    let manual = s.manual.read().await;
    let mine = |r: &str| q.repo.as_deref().is_none_or(|q| r == q);
    let conventions = crate::conventions::apply(s.data.conventions.clone(), &manual)
        .into_iter()
        .filter(|c| mine(&c.repo))
        .map(|c| api::ShownConvention {
            id: c.id(),
            convention: c,
        })
        .collect();
    let hidden = manual
        .iter()
        .filter(|m| m.hidden && mine(&m.repo))
        .cloned()
        .collect();
    Json(api::Conventions {
        conventions,
        hidden,
    })
}

async fn change_convention(
    who: Member,
    Cur(s): Cur,
    Json(b): Json<api::ConventionChange>,
) -> Reply<api::Done> {
    match apply_convention(&s, b, &identity(who.as_deref()).actor).await {
        Ok(msg) => Ok(Json(api::Done::new(msg))),
        Err(e) => Err(bad(e)),
    }
}

async fn apply_convention(s: &Session, b: api::ConventionChange, actor: &str) -> Result<String> {
    use crate::conventions::{change, Change, Manual, Write, ORG};
    let root = s.data.root.as_deref().context("read-only UI")?;
    let mut manual = s.manual.write().await;
    let w = if b.action == "add" {
        let dimension = b.dimension.as_deref().unwrap_or("team");
        Write::Put(Manual::new(
            b.repo.as_deref().unwrap_or(ORG),
            dimension,
            &b.rule,
            actor,
        ))
    } else {
        let what = match b.action.as_str() {
            "edit" => Change::Edit(b.rule),
            "hide" => Change::Hide,
            "restore" => Change::Restore,
            other => anyhow::bail!("unknown action {other}"),
        };
        let current = crate::conventions::apply(s.data.conventions.clone(), &manual);
        change(&current, &manual, &b.id, what, actor)?
    };
    crate::conventions::Store::open(root)?.write(&w).await?;
    Ok(match w {
        Write::Put(m) => {
            let id = m.id.clone();
            manual.retain(|x| x.id != id);
            let verb = if m.hidden { "hid" } else { "saved" };
            manual.push(m);
            format!("{verb} {id}")
        }
        Write::Delete(id) => {
            manual.retain(|x| x.id != id);
            format!("removed {id}")
        }
    })
}

async fn find(Cur(s): Cur, Query(q): Query<api::FindQuery>) -> Json<Vec<Node>> {
    Json(s.data.graph.find(&q.q, 40))
}

async fn node(Cur(s): Cur, Query(q): Query<api::FocusQuery>) -> Reply<api::NodeDetail> {
    let g = &s.data.graph;
    let Some(idx) = g.lookup(&q.focus) else {
        return Err(failure(StatusCode::NOT_FOUND, "no such symbol"));
    };
    let z = g.zoom(idx);
    let ripple = g.ripple(idx, 4);
    let mut repos: BTreeMap<String, usize> = BTreeMap::new();
    for h in &ripple {
        *repos.entry(h.node.repo.clone()).or_default() += 1;
    }
    let mut ids = vec![q.focus.clone()];
    if let Some(f) = &z.focus {
        ids.push(f.id.clone());
        if let Some((_, scope)) = g.scope_of(&f.id) {
            ids.extend(scope);
        }
    }
    let extras = s.extras().await;
    let fid = z.focus.as_ref().map(|f| f.id.clone()).unwrap_or_default();
    let summary = extras.enrichment.get(&fid).map(|(s, _)| s.clone());
    let flow = extras.flows.get(&fid).map(|(n, q)| api::Narration {
        narration: n.clone(),
        questions: q.clone(),
    });
    let entries = s.entries().await;
    let about: Vec<&Entry> = crate::ledger::about(&entries, &ids);
    Ok(Json(api::NodeDetail {
        graph: neighbourhood(&z),
        zoom: z,
        ripple: api::Ripple {
            total: ripple.len(),
            repos,
        },
        ledger: named(&s, &about),
        summary,
        flow,
    }))
}

/// Hybrid code search (BM25, vectors, centrality), as `comindly search` and the MCP tool.
async fn search(Cur(s): Cur, Query(q): Query<api::SearchQuery>) -> Reply<Vec<api::SearchHit>> {
    let run = async {
        let tables = s.tables.as_ref().context("no index root")?;
        let engine = s
            .search()
            .await
            .context("the embedding model could not be loaded")?;
        let extras = s.extras().await;
        let limit = if q.repo.is_some() { 60 } else { 20 };
        let mut hits = crate::search::hybrid(
            tables,
            &engine.by_id,
            &s.data.graph,
            &extras.enrichment,
            &engine.embedder,
            &q.q,
            limit,
        )
        .await
        .map_err(|e| match format!("{e:#}").contains("was not found") {
            true => {
                anyhow::anyhow!("this index has no search table: rebuild it without --no-embed")
            }
            false => e.context("hybrid search failed"),
        })?;
        if let Some(r) = &q.repo {
            hits.retain(|h| &h.repo == r);
            hits.truncate(20);
        }
        anyhow::Ok(
            hits.into_iter()
                .map(|h| api::SearchHit {
                    id: h.id,
                    name: h.name,
                    kind: h.kind,
                    repo: h.repo,
                    location: h.location,
                    signature: h.signature,
                    summary: h.summary,
                    dependents: h.deps,
                })
                .collect(),
        )
    };
    run.await.map(Json).map_err(bad)
}

/// The context pack for a symbol as markdown, ready to paste into an agent; plain text, not JSON.
async fn context(Cur(s): Cur, Query(q): Query<api::FocusQuery>) -> impl IntoResponse {
    let g = &s.data.graph;
    let Some(idx) = g.lookup(&q.focus) else {
        return (StatusCode::NOT_FOUND, "no such symbol".to_string());
    };
    let extras = s.extras().await;
    let focus = g.node(idx);
    let pack = g.context_pack(idx, q.budget.unwrap_or(1500));
    let mut o = format!(
        "## Context for changing `{}` ({} · {})\n\nRead these first:\n\n",
        focus.name, focus.repo, focus.location
    );
    for (n, _) in pack.iter().filter(|(n, _)| n.id != focus.id) {
        o.push_str(&format!("- `{}` ({} · {})", n.name, n.repo, n.location));
        if let Some((sum, _)) = extras.enrichment.get(&n.id) {
            o.push_str(&format!(": {sum}"));
        }
        o.push('\n');
    }
    let mut ids: Vec<String> = pack.iter().map(|(n, _)| n.id.clone()).collect();
    ids.push(focus.id.clone());
    let entries = s.entries().await;
    let known: Vec<String> = crate::ledger::about(&entries, &ids)
        .into_iter()
        .filter(|e| e.trust >= crate::ledger::Trust::T2)
        .filter(|e| !matches!(e.status, Status::Superseded | Status::Retracted))
        .take(6)
        .map(crate::ledger::line)
        .collect();
    if !known.is_empty() {
        o.push_str("\nRecorded by the team:\n\n");
        for k in known {
            o.push_str(&format!("- {k}\n"));
        }
    }
    (StatusCode::OK, o)
}

async fn packages(Cur(s): Cur) -> Json<Vec<api::Package>> {
    Json(
        s.data
            .deps
            .iter()
            .map(|d| api::Package {
                ecosystem: d.ecosystem.clone(),
                name: d.name.clone(),
                publisher: d.publisher.clone(),
                published: d.published.clone(),
                consumer: d.consumer.clone(),
                requirement: d.requirement.clone(),
                locked: d.locked.clone(),
                skewed: d.skewed(),
            })
            .collect(),
    )
}

/// Setup status and how to connect agents: index location, checks, MCP tools.
async fn setup(State(app): State<Shared>, Cur(s): Cur) -> Json<api::Setup> {
    let (server, team) = (app.server.clone(), app.team());
    let root = s.data.root.clone().unwrap_or_default();
    let checks = {
        let (root, server) = (root.clone(), server.clone());
        tokio::task::spawn_blocking(move || crate::doctor::run(&team, server.as_deref(), &root))
            .await
            .unwrap_or_default()
    };
    let tools = crate::mcp::tools()
        .into_iter()
        .map(|(name, description)| api::Tool { name, description })
        .collect();
    let agents = match server {
        Some(_) => Vec::new(),
        None => crate::doctor::agents()
            .into_iter()
            .map(String::from)
            .collect(),
    };
    Json(api::Setup {
        remote: crate::cache::is_remote(&root),
        index: root,
        version: env!("CARGO_PKG_VERSION").into(),
        checks,
        tools,
        agents,
    })
}

/// Nodes and labelled edges around the focus, for the canvas graph.
fn neighbourhood(z: &Zoom) -> api::Neighbourhood {
    let Some(f) = &z.focus else {
        return api::Neighbourhood::default();
    };
    let mut nodes: BTreeMap<&str, &Node> = BTreeMap::from([(f.id.as_str(), f)]);
    let mut edges: Vec<api::GraphEdge> = Vec::new();
    let mut add = |a: &Node, b: &Node, kind: &str| {
        edges.push(api::GraphEdge {
            src: a.id.clone(),
            dst: b.id.clone(),
            kind: kind.into(),
        });
    };
    const CAP: usize = 40;
    let groups: [(&[Node], bool, &str); 5] = [
        (&z.callers, false, "calls"),
        (&z.callees, true, "calls"),
        (&z.importers, false, "imports"),
        (&z.members, true, "contains"),
        (&z.consumers, false, "consumes"),
    ];
    for (list, outgoing, kind) in groups {
        for n in list.iter().take(CAP) {
            nodes.insert(&n.id, n);
            if outgoing {
                add(f, n, kind);
            } else {
                add(n, f, kind);
            }
        }
    }
    for p in z.providers.iter().take(CAP) {
        nodes.insert(&p.id, p);
        add(p, f, "provides");
    }
    if let Some(c) = &z.container {
        nodes.insert(&c.id, c);
        add(c, f, "contains");
    }
    for r in &z.contracts {
        nodes.insert(&r.contract.id, &r.contract);
        add(f, &r.contract, r.role.as_str());
        for c in r.counterparts.iter().take(CAP) {
            nodes.insert(&c.id, c);
            add(c, &r.contract, "counterpart");
        }
    }
    api::Neighbourhood {
        nodes: nodes.into_values().cloned().collect(),
        edges,
        focus: Some(f.id.clone()),
    }
}

/// Provenance for a write from the UI: the signed-in member on a server, else this machine's user.
fn provenance(who: Option<&crate::server::Who>) -> Provenance {
    Provenance::human("comindly-web", &identity(who))
}

/// The signed-in member on a server, else this machine's user.
fn identity(who: Option<&crate::server::Who>) -> crate::ledger::Identity {
    match who {
        Some(w) => crate::ledger::Identity {
            actor: format!("actor-{}", &blake3::hash(w.email.as_bytes()).to_hex()[..10]),
            name: Some(w.email.clone()),
        },
        None => crate::config::local_identity(),
    }
}

type Member = Option<axum::Extension<crate::server::Who>>;

/// Records written here are signed with this machine's key in local mode; a server does not sign.
fn signer(who: Option<&crate::server::Who>) -> Option<std::path::PathBuf> {
    who.is_none()
        .then(crate::config::local_signing_key)
        .flatten()
}

async fn review(who: Member, Cur(s): Cur, Json(b): Json<api::ReviewRequest>) -> Reply<api::Done> {
    let signer = signer(who.as_deref());
    match apply_review(&s, &b, provenance(who.as_deref()), signer).await {
        Ok(msg) => Ok(Json(api::Done::new(msg))),
        Err(e) => Err(bad(e)),
    }
}

async fn apply_review(
    s: &Session,
    b: &api::ReviewRequest,
    prov: Provenance,
    signer: Option<std::path::PathBuf>,
) -> Result<String> {
    let root = s.data.root.as_deref().context("read-only UI")?;
    let (kind, _) = crate::ledger::review_action(&b.action).context("unknown action")?;
    let ledger = crate::ledger::Ledger::open(root)?.signed_by(signer);
    let target = {
        let entries = s.entries().await;
        let exact = entries.iter().find(|e| e.record.id == b.id);
        let prefixed: Vec<_> = entries
            .iter()
            .filter(|e| e.record.id.starts_with(&b.id))
            .collect();
        match (exact, prefixed.as_slice()) {
            (Some(e), _) | (None, &[e]) => e.record.clone(),
            (None, []) => anyhow::bail!("no ledger record `{}`", b.id),
            (None, _) => anyhow::bail!("`{}` matches several records; give more of the id", b.id),
        }
    };
    if matches!(kind, RelationKind::Retracts) && target.kind == crate::ledger::Kind::Relation {
        anyhow::bail!("relations cannot be retracted");
    }
    let rec = Record::review(&target, &b.action, &b.reason, prov)?;
    ledger.put(&rec).await?;
    let records = ledger.all().await?;
    let fresh = crate::ledger::derive(&records, &s.data.symbols, &crate::config::local_verifier());
    let status = fresh
        .iter()
        .find(|e| e.record.id == target.id)
        .map_or(Status::Active, |e| e.status);
    *s.entries.write().await = fresh;
    Ok(format!("{} → {:?}", target.title, status))
}

/// A record written by the person using the UI.
async fn remember(
    who: Member,
    Cur(s): Cur,
    Json(b): Json<api::RememberRequest>,
) -> Reply<api::Remembered> {
    let run = async {
        let root = s.data.root.as_deref().context("read-only UI")?;
        let kind = crate::ledger::Kind::parse(&b.kind).context("unknown kind")?;
        let g = &s.data.graph;
        let mut anchors = Vec::new();
        for focus in &b.on {
            let id = g
                .lookup(focus)
                .map(|i| g.node(i).id)
                .with_context(|| format!("no symbol matches `{focus}`"))?;
            let sym = s.data.symbols.iter().find(|x| x.id.render() == id);
            anchors.push(crate::ledger::Anchor::of(sym.context("symbol not loaded")?));
        }
        let mut rec = Record::new(kind, &b.title, anchors, provenance(who.as_deref()));
        rec.body = b.body.trim().to_string();
        rec.trigger = b.trigger.filter(|t| !t.trim().is_empty());
        rec.evidence = b
            .evidence
            .into_iter()
            .filter(|e| !e.trim().is_empty())
            .collect();
        for (k, v) in b.fields.into_iter().filter(|(_, v)| !v.trim().is_empty()) {
            rec.fields.insert(k, Value::String(v));
        }
        let ledger = crate::ledger::Ledger::open(root)?.signed_by(signer(who.as_deref()));
        ledger.put(&rec).await?;
        let records = ledger.all().await?;
        *s.entries.write().await =
            crate::ledger::derive(&records, &s.data.symbols, &crate::config::local_verifier());
        anyhow::Ok(rec.id)
    };
    match run.await {
        Ok(id) => Ok(Json(api::Remembered { ok: true, id })),
        Err(e) => Err(bad(e)),
    }
}

/// Permanently delete a record and the reviews about it.
async fn erase(Cur(s): Cur, Json(b): Json<api::IdRequest>) -> Reply<api::Done> {
    let run = async {
        let root = s.data.root.as_deref().context("read-only UI")?;
        let ledger = crate::ledger::Ledger::open(root)?;
        let gone = ledger.erase(|r| r.id == b.id, true).await?;
        anyhow::ensure!(!gone.is_empty(), "no such record");
        let records = ledger.all().await?;
        *s.entries.write().await =
            crate::ledger::derive(&records, &s.data.symbols, &crate::config::local_verifier());
        anyhow::Ok(format!("deleted {} record(s)", gone.len()))
    };
    match run.await {
        Ok(msg) => Ok(Json(api::Done::new(msg))),
        Err(e) => Err(bad(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A router behind `traced` whose log lines, as JSON, land in the returned buffer.
    fn traced_app() -> (Router, Arc<std::sync::Mutex<Vec<String>>>) {
        let lines: Arc<std::sync::Mutex<Vec<String>>> = Default::default();
        let app = Router::new()
            .route(
                "/api/v1/thing",
                get(|rid: axum::Extension<RequestId>| async move {
                    crate::ui::warn("something odd");
                    let mut res = rid.0 .0.into_response();
                    res.extensions_mut().insert(crate::server::Who {
                        email: "member@example.com".into(),
                        role: crate::server::Role::Member,
                    });
                    res
                }),
            )
            .layer(axum::middleware::from_fn_with_state(false, traced));
        (app, lines)
    }

    async fn call(app: Router, req: axum::http::Request<axum::body::Body>) -> (String, String) {
        use tower::ServiceExt;
        let res = app.oneshot(req).await.unwrap();
        let id = res.headers()["x-request-id"].to_str().unwrap().to_string();
        let body = axum::body::to_bytes(res.into_body(), 1 << 16)
            .await
            .unwrap();
        (id, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn requests_get_an_id_and_one_access_line_without_credentials() {
        use tracing_subscriber::layer::SubscriberExt;
        let (app, lines) = traced_app();
        let out = lines.clone();
        let sub = tracing_subscriber::registry()
            .with(crate::telemetry::Lines::to(true, move |l| {
                out.lock().unwrap().push(l.to_string())
            }));
        let _g = tracing::subscriber::set_default(sub);

        let req = axum::http::Request::get("/api/v1/thing?token=query-secret")
            .header("x-request-id", "abc-123.x_y")
            .header(header::AUTHORIZATION, "Bearer bearer-secret")
            .header(header::COOKIE, "comindly_session=cookie-secret")
            .body(axum::body::Body::empty())
            .unwrap();
        let (id, seen) = call(app.clone(), req).await;
        assert_eq!(id, "abc-123.x_y");
        assert_eq!(seen, id, "the handler sees the same id");

        let lines = lines.lock().unwrap().clone();
        let all = lines.join("\n");
        for secret in ["query-secret", "bearer-secret", "cookie-secret", "token="] {
            assert!(!all.contains(secret), "{secret} leaked: {all}");
        }
        let parsed: Vec<Value> = lines
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let warn = parsed.iter().find(|v| v["kind"] == "warning").unwrap();
        assert_eq!(warn["request_id"], "abc-123.x_y");
        let access: Vec<&Value> = parsed.iter().filter(|v| v["kind"] == "access").collect();
        assert_eq!(access.len(), 1);
        let a = access[0];
        assert_eq!(a["level"], "info");
        assert_eq!(a["method"], "GET");
        assert_eq!(a["path"], "/api/v1/thing");
        assert_eq!(a["status"], 200);
        assert_eq!(a["request_id"], "abc-123.x_y");
        assert_eq!(a["email"], "member@example.com");
        assert!(a["duration_ms"].is_u64());

        for bad in ["has space", "semi;colon", &"x".repeat(65)] {
            let req = axum::http::Request::get("/api/v1/thing")
                .header("x-request-id", bad)
                .body(axum::body::Body::empty())
                .unwrap();
            let (id, _) = call(app.clone(), req).await;
            assert_ne!(id, bad);
            assert!(safe_request_id(&id));
        }
        let req = axum::http::Request::get("/api/v1/thing")
            .body(axum::body::Body::empty())
            .unwrap();
        let (id, _) = call(app, req).await;
        assert!(safe_request_id(&id));
    }

    fn n(id: &str, repo: &str) -> Node {
        Node {
            id: id.into(),
            name: id.into(),
            kind: "function".into(),
            repo: repo.into(),
            location: String::new(),
            signature: None,
        }
    }

    #[test]
    fn neighbourhood_links_focus_both_ways() {
        let z = Zoom {
            focus: Some(n("f", "a")),
            callers: vec![n("caller", "b")],
            callees: vec![n("callee", "a")],
            ..Default::default()
        };
        let g = neighbourhood(&z);
        assert_eq!(g.nodes.len(), 3);
        let edges: Vec<(String, String)> = g
            .edges
            .iter()
            .map(|e| (e.src.clone(), e.dst.clone()))
            .collect();
        assert!(edges.contains(&("caller".into(), "f".into())));
        assert!(edges.contains(&("f".into(), "callee".into())));
    }
}
