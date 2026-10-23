//! Server mode in the web UI: sign-in by email link, invitations, members, personal access tokens
//! for agents, and the tokens and keys the server uses.

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};

use std::sync::Arc;

use super::{bad, failure as fail, Reply, Shared};
use crate::api::{self, Done};
use crate::server::{self, Role, Server, ServerConfig, SignIn, Via, Who};

/// The server, for routes that exist only in server mode.
pub(super) struct On(Arc<Server>);

impl axum::extract::FromRequestParts<Shared> for On {
    type Rejection = Response;

    async fn from_request_parts(
        _: &mut axum::http::request::Parts,
        app: &Shared,
    ) -> Result<Self, Self::Rejection> {
        app.server.clone().map(On).ok_or_else(unavailable)
    }
}

/// The session cookie; on HTTPS with the `__Host-` prefix, so no sibling domain can set it.
fn cookie_name(cfg: &ServerConfig) -> &'static str {
    if cfg.https() {
        "__Host-comindly_session"
    } else {
        "comindly_session"
    }
}

fn unavailable() -> Response {
    super::error_body(
        StatusCode::SERVICE_UNAVAILABLE,
        "the server's database could not be opened; see its log",
    )
}

fn cookie(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|c| {
            c.trim()
                .strip_prefix(&format!("{name}="))
                .map(str::to_string)
        })
}

fn bearer(req: &Request) -> Option<String> {
    req.headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

/// `scheme://host:port` of a URL, lower-case and with the scheme's default port spelled out.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let authority = rest.split(['/', '?', '#']).next()?.to_ascii_lowercase();
    let authority = authority.rsplit('@').next()?;
    let port = match scheme.as_str() {
        "https" => "443",
        "http" => "80",
        _ => return None,
    };
    let full = match authority.rsplit_once(':') {
        Some((_, p)) if p.chars().all(|c| c.is_ascii_digit()) => authority.to_string(),
        _ => format!("{authority}:{port}"),
    };
    Some(format!("{scheme}://{full}"))
}

/// A write made with the session cookie must come from this site's own pages.
fn same_site(req: &Request, cfg: &ServerConfig) -> bool {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok());
    match origin {
        Some(o) => origin_of(o).is_some_and(|o| Some(o) == origin_of(&cfg.public_url)),
        None => req
            .headers()
            .get("sec-fetch-site")
            .is_some_and(|v| v == "same-origin"),
    }
}

/// Where a personal access token revokes itself, the one account path a token may call.
pub(super) const SELF_REVOKE: &str = "/api/v1/auth/token/revoke";

/// What a personal access token may not do: manage the account, its tokens, members or secrets.
fn account_only(path: &str) -> bool {
    [
        "/api/v1/auth/",
        "/api/v1/tokens",
        "/api/v1/members",
        "/api/v1/secrets",
        "/api/v1/audit",
    ]
    .iter()
    .any(|p| path.starts_with(p))
}

/// The least role a request needs: viewers read, members also write to the ledger and
/// conventions, admins manage people, secrets, repositories, settings and indexing.
fn needs(method: &Method, path: &str) -> Role {
    if ["/api/v1/members", "/api/v1/secrets", "/api/v1/audit"]
        .iter()
        .any(|p| path.starts_with(p))
    {
        return Role::Admin;
    }
    if method == Method::GET
        || path.starts_with("/api/v1/auth/")
        || path.starts_with("/api/v1/tokens")
        || path.starts_with("/api/v1/answer/")
    {
        return Role::Viewer;
    }
    if ["/api/v1/remember", "/api/v1/review", "/api/v1/conventions"]
        .iter()
        .any(|p| path.starts_with(p))
    {
        return Role::Member;
    }
    Role::Admin
}

/// Who may do what in server mode: the session cookie or a bearer token decides.
pub(super) async fn guard(
    axum::extract::State(server): axum::extract::State<Arc<Server>>,
    mut req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    if !(path.starts_with("/api/") || path.starts_with("/mcp")) {
        return next.run(req).await;
    }
    let open = matches!(
        path.as_str(),
        "/api/v1/auth/me"
            | "/api/v1/auth/link"
            | "/api/v1/auth/redeem"
            | "/api/v1/auth/code"
            | "/api/v1/auth/device/start"
            | "/api/v1/auth/device/token"
    );
    let token = bearer(&req);
    let session = cookie(&req, cookie_name(&server.config));
    let found = tokio::task::block_in_place(|| {
        server
            .auth
            .authenticate(session.as_deref(), token.as_deref())
            .unwrap_or(None)
    });
    let signed = found.as_ref().map(|(w, _)| w.clone());
    if let Some(w) = &signed {
        req.extensions_mut().insert(w.clone());
    }
    if open {
        return by(next.run(req).await, signed);
    }
    let Some((who, via)) = found else {
        if token.is_some() && session.is_none() {
            return fail(
                StatusCode::UNAUTHORIZED,
                "This access token has expired or was revoked",
            )
            .into_response();
        }
        return fail(StatusCode::UNAUTHORIZED, "Sign in to continue").into_response();
    };
    if via == Via::Token && account_only(&path) && path != SELF_REVOKE {
        return fail(
            StatusCode::FORBIDDEN,
            "Personal access tokens are for agents; sign in to manage your account",
        )
        .into_response();
    }
    if via == Via::Session && req.method() != Method::GET && !same_site(&req, &server.config) {
        return fail(
            StatusCode::FORBIDDEN,
            "This request did not come from comindly's own pages",
        )
        .into_response();
    }
    // Agents write over `/mcp`; its tools check the role themselves.
    let need = if path.starts_with("/mcp") {
        Role::Viewer
    } else {
        needs(req.method(), &path)
    };
    if !who.role.at_least(need) {
        let msg = match need {
            Role::Admin => "Only admins can do this",
            _ => "Viewers can read, not change anything",
        };
        return fail(StatusCode::FORBIDDEN, msg).into_response();
    }
    by(next.run(req).await, signed)
}

/// Tells the access log who made the request.
fn by(mut res: Response, who: Option<Who>) -> Response {
    if let Some(w) = who {
        res.extensions_mut().insert(w);
    }
    res
}

/// The names this server answers to: its public address, and localhost for checks on the host.
pub(super) fn hosts(cfg: &ServerConfig) -> Vec<String> {
    let mut out: Vec<String> = ["localhost", "127.0.0.1", "::1"].map(String::from).to_vec();
    let url = &cfg.public_url;
    let authority = url.split("://").nth(1).unwrap_or(url);
    let authority = authority.split('/').next().unwrap_or(authority);
    out.push(authority.to_string());
    out.push(authority.split(':').next().unwrap_or(authority).to_string());
    out
}

pub(super) async fn me(
    super::Srv(server): super::Srv,
    who: Option<Extension<Who>>,
) -> Json<api::Me> {
    let Some(server) = server else {
        return Json(api::Me::Local);
    };
    Json(api::Me::Server {
        signed_in: who.is_some(),
        email: who.as_ref().map(|w| w.email.clone()),
        role: who.as_ref().map(|w| w.role),
        sign_in: server.config.sign_in,
        public_url: server.config.public_url.clone(),
    })
}

fn link(cfg: &ServerConfig, path: &str, token: &str) -> String {
    format!("{}/{path}?t={token}", cfg.public_url)
}

/// Email a sign-in link; the answer is the same whether or not the address is a member.
pub(super) async fn request_link(On(server): On, Json(q): Json<api::EmailRequest>) -> Reply<Done> {
    if server.config.sign_in != SignIn::Email {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "This server signs in with codes",
        ));
    }
    let email = server::normalize(&q.email);
    // Sent in the background, so the answer neither waits on the mail server nor reveals a member.
    match server.accounts.sign_in_link(&email) {
        Ok(Some(token)) => {
            let body = format!(
                "Sign in to comindly with this link (valid for 15 minutes, once):\n\n{}\n\nIf you did not ask for it, ignore this email.",
                link(&server.config, "signin", &token)
            );
            let server = server.clone();
            tokio::spawn(async move {
                if let Err(e) = server
                    .mailer
                    .send(&email, "Your comindly sign-in link", &body)
                    .await
                {
                    crate::ui::warn(&super::error_text(&e));
                }
            });
        }
        Ok(None) => {}
        Err(e) => crate::ui::warn(&format!("sign-in link: {}", super::error_text(&e))),
    }
    Ok(Json(Done::new(
        "If that address belongs to a member, a sign-in link is on its way",
    )))
}

fn session_cookie(cfg: &ServerConfig, token: &str, max_age: i64) -> String {
    format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        cookie_name(cfg),
        if cfg.https() { "; Secure" } else { "" }
    )
}

/// Exchange a sign-in link, an invitation or the admin token for a session cookie.
pub(super) async fn redeem(
    On(server): On,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(q): Json<api::RedeemRequest>,
) -> Response {
    let cfg = &server.config;
    match server
        .accounts
        .redeem(&q.token, &client(cfg, peer, &headers))
    {
        Ok((session, who)) => (
            StatusCode::OK,
            [(
                header::SET_COOKIE,
                session_cookie(cfg, &session, 30 * 86400),
            )],
            Json(Done::new(format!("Signed in as {}", who.email))),
        )
            .into_response(),
        Err(e) => bad(e).into_response(),
    }
}

/// Where a request comes from: the connecting address, or behind a proxy you run
/// (`COMINDLY_TRUST_PROXY=true`) the address that proxy added last to `X-Forwarded-For`.
fn client(
    cfg: &ServerConfig,
    peer: std::net::SocketAddr,
    headers: &axum::http::HeaderMap,
) -> String {
    let forwarded = cfg
        .trust_proxy
        .then(|| {
            headers
                .get("x-forwarded-for")?
                .to_str()
                .ok()?
                .rsplit(',')
                .next()
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
        })
        .flatten();
    forwarded.unwrap_or_else(|| peer.ip().to_string())
}

/// Sign in with an email address and a code; an invitation code also returns the person's own
/// sign-in code, once.
pub(super) async fn sign_in_code(
    On(server): On,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(q): Json<api::CodeRequest>,
) -> Response {
    let cfg = &server.config;
    if cfg.sign_in != SignIn::Code {
        return fail(StatusCode::BAD_REQUEST, "This server signs in by email").into_response();
    }
    match server
        .accounts
        .sign_in_code(&q.email, &q.code, &client(cfg, peer, &headers))
    {
        Ok((session, who, code)) => (
            StatusCode::OK,
            [(
                header::SET_COOKIE,
                session_cookie(cfg, &session, 30 * 86400),
            )],
            Json(api::WithCode {
                ok: true,
                message: format!("Signed in as {}", who.email),
                code,
            }),
        )
            .into_response(),
        Err(e) => bad(e).into_response(),
    }
}

/// A new sign-in code for yourself; your other sessions end.
pub(super) async fn reset_own_code(
    On(server): On,
    Extension(who): Extension<Who>,
    req: Request,
) -> Reply<api::WithCode> {
    if server.config.sign_in != SignIn::Code {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "This server signs in by email",
        ));
    }
    let mine = cookie(&req, cookie_name(&server.config));
    let code = server
        .accounts
        .reset_code(&who.email, &who.email, mine.as_deref())
        .map_err(bad)?;
    Ok(Json(api::WithCode {
        ok: true,
        message: "New sign-in code; the old one no longer works".into(),
        code: Some(code),
    }))
}

pub(super) async fn sign_out(On(server): On, req: Request) -> Response {
    let everywhere = req
        .uri()
        .query()
        .is_some_and(|q| q.contains("everywhere=true"));
    if let Some(s) = cookie(&req, cookie_name(&server.config)) {
        let _ = server.accounts.sign_out(&s, everywhere);
    }
    (
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie(&server.config, "", 0))],
        Json(Done::new("Signed out")),
    )
        .into_response()
}

pub(super) async fn members(On(server): On) -> Reply<api::Members> {
    let store = &server.accounts;
    match (store.members(), store.invites()) {
        (Ok(members), Ok(invites)) => Ok(Json(api::Members {
            members,
            invites,
            sign_in: server.config.sign_in,
        })),
        (Err(e), _) | (_, Err(e)) => Err(bad(e)),
    }
}

/// Invite `email`: by email, or with a code the admin hands over (returned once).
pub(super) async fn invite(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::MemberRole>,
) -> Reply<api::Invited> {
    let store = &server.accounts;
    let email = server::normalize(&q.email);
    let code = server.config.sign_in == SignIn::Code;
    let token = store
        .invite(&who.email, &email, q.role, code)
        .map_err(bad)?;
    if code {
        return Ok(Json(api::Invited {
            ok: true,
            message: format!("Invited {email}; give them this code"),
            code: Some(token),
        }));
    }
    let body = format!(
        "{} invited you to comindly. Join with this link (valid for 7 days, once, for {email} only):\n\n{}",
        who.email,
        link(&server.config, "signin", &token)
    );
    match server
        .mailer
        .send(&email, "You're invited to comindly", &body)
        .await
    {
        Ok(()) => Ok(Json(api::Invited {
            ok: true,
            message: format!("Invitation emailed to {email}"),
            code: None,
        })),
        Err(e) => {
            let _ = store.revoke_invite(&who.email, &email);
            Err(bad(e))
        }
    }
}

/// A new sign-in code for a member who lost theirs; their sessions end.
pub(super) async fn reset_code(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::EmailRequest>,
) -> Reply<api::WithCode> {
    if server.config.sign_in != SignIn::Code {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "This server signs in by email",
        ));
    }
    let code = server
        .accounts
        .reset_code(&who.email, &q.email, None)
        .map_err(bad)?;
    Ok(Json(api::WithCode {
        ok: true,
        message: "New sign-in code; give it to them".into(),
        code: Some(code),
    }))
}

pub(super) async fn set_role(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::MemberRole>,
) -> Reply<Done> {
    done(
        server.accounts.set_role(&who.email, &q.email, q.role),
        "Role changed",
    )
}

pub(super) async fn remove(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::EmailRequest>,
) -> Reply<Done> {
    if server::normalize(&q.email) == who.email {
        return Err(fail(StatusCode::BAD_REQUEST, "You cannot remove yourself"));
    }
    done(
        server.accounts.remove(&who.email, &q.email),
        "Member removed",
    )
}

pub(super) async fn revoke_invite(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::EmailRequest>,
) -> Reply<Done> {
    done(
        server.accounts.revoke_invite(&who.email, &q.email),
        "Invitation withdrawn",
    )
}

fn done(r: anyhow::Result<()>, message: &str) -> Reply<Done> {
    r.map(|()| Json(Done::new(message))).map_err(bad)
}

pub(super) async fn tokens(On(server): On, Extension(who): Extension<Who>) -> Reply<api::Tokens> {
    let tokens = server.accounts.tokens(&who).map_err(bad)?;
    Ok(Json(api::Tokens { tokens }))
}

/// A personal access token for an agent; shown once.
pub(super) async fn create_token(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::NewToken>,
) -> Reply<api::TokenCreated> {
    let token = server
        .accounts
        .create_token(&who, &q.name, q.expires_in_days)
        .map_err(bad)?;
    Ok(Json(api::TokenCreated {
        ok: true,
        message: "Token created; copy it now, it is shown once".into(),
        token,
    }))
}

pub(super) async fn revoke_token(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::IdRequest>,
) -> Reply<Done> {
    done(server.accounts.revoke_token(&who, &q.id), "Token revoked")
}

/// The audit log, newest first; `before` takes the previous page's `next`.
pub(super) async fn audit(
    On(server): On,
    axum::extract::Query(q): axum::extract::Query<api::AuditQuery>,
) -> Reply<api::AuditPage> {
    tokio::task::block_in_place(|| server.audit.audit_entries(&q))
        .map(Json)
        .map_err(bad)
}

/// Revoke the personal access token this request was made with, as `comindly logout` does.
pub(super) async fn revoke_self(On(server): On, req: Request) -> Reply<Done> {
    let only = || {
        fail(
            StatusCode::BAD_REQUEST,
            "Only a personal access token can revoke itself",
        )
    };
    let token = bearer(&req).ok_or_else(only)?;
    match server.accounts.revoke_own_token(&token) {
        Ok(true) => Ok(Json(Done::new("Token revoked"))),
        Ok(false) => Err(only()),
        Err(e) => Err(bad(e)),
    }
}

/// The deployment's value for a secret, which always wins over one stored here.
fn deployment(cfg: &ServerConfig, name: &str) -> Option<String> {
    if name == server::LLM_SECRET {
        return cfg.llm_key.clone();
    }
    let host = name.strip_prefix("git:")?;
    crate::config::env_token(host)
}

fn label(name: &str) -> String {
    match name.strip_prefix("git:") {
        Some(h) if h.contains("github") => format!("GitHub token for {h}"),
        Some(h) if h.contains("gitlab") => format!("GitLab token for {h}"),
        Some(h) => format!("Git token for {h}"),
        None => "LLM API key".into(),
    }
}

/// The server's tokens and keys: where each comes from, never their values.
pub(super) async fn secrets(On(server): On) -> Json<api::Secrets> {
    let store = &server.secrets;
    let mut names = vec![
        server::LLM_SECRET.to_string(),
        server::git_secret("github.com"),
        server::git_secret("gitlab.com"),
    ];
    for n in store.secret_names() {
        if !names.contains(&n) {
            names.push(n);
        }
    }
    let secrets = names
        .iter()
        .map(|n| {
            let elsewhere = |source: &str| {
                api::SecretState::Elsewhere(api::SecretSource {
                    source: source.into(),
                    name: n.clone(),
                    label: label(n),
                })
            };
            if deployment(&server.config, n).is_some() {
                elsewhere("deployment")
            } else if let Some(mut info) = store.secret_info(n) {
                info.name = n.clone();
                api::SecretState::Stored {
                    info,
                    label: label(n),
                }
            } else {
                elsewhere("none")
            }
        })
        .collect();
    Json(api::Secrets {
        secrets,
        can_store: true,
    })
}

/// Store or remove a token or key; one the deployment provides cannot be changed here.
pub(super) async fn set_secret(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::SecretRequest>,
) -> Reply<Done> {
    let name = q.name.trim().to_lowercase();
    let valid = name == server::LLM_SECRET
        || name.strip_prefix("git:").is_some_and(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
        });
    if !valid {
        return Err(fail(StatusCode::BAD_REQUEST, "Unknown secret"));
    }
    if deployment(&server.config, &name).is_some() {
        return Err(fail(
            StatusCode::BAD_REQUEST,
            "The deployment provides this one; change it where the deployment keeps its secrets",
        ));
    }
    let message = if q.value.trim().is_empty() {
        "Removed"
    } else {
        "Saved"
    };
    done(
        server.secrets.set_secret(&who.email, &name, &q.value),
        message,
    )
}

/// A command line asks to sign in; a member approves it at the returned address.
pub(super) async fn device_start(
    On(server): On,
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(q): Json<api::DeviceStartRequest>,
) -> Reply<api::DeviceStart> {
    let from = client(&server.config, peer, &headers);
    let (device_code, user_code) = server.devices.device_start(&q.client, &from).map_err(bad)?;
    Ok(Json(api::DeviceStart {
        device_code,
        verify_url: format!("{}/device?code={user_code}", server.config.public_url),
        user_code,
        interval: 2,
        expires_in: 600,
    }))
}

/// What the command line polls: pending, or the token once a member approved.
pub(super) async fn device_token(
    On(server): On,
    Json(q): Json<api::DeviceTokenRequest>,
) -> Reply<api::DeviceToken> {
    match server.devices.device_poll(&q.device_code) {
        Ok(server::DevicePoll::Pending) => Ok(Json(api::DeviceToken::Pending)),
        Ok(server::DevicePoll::Approved {
            token,
            email,
            expires,
        }) => Ok(Json(api::DeviceToken::Approved {
            token,
            email,
            expires,
        })),
        Ok(server::DevicePoll::Denied) => {
            Err(fail(StatusCode::FORBIDDEN, "The sign-in was denied"))
        }
        Ok(server::DevicePoll::Expired) => Err(fail(
            StatusCode::GONE,
            "The sign-in expired or was already used; start it again",
        )),
        Err(e) => Err(bad(e)),
    }
}

pub(super) async fn device_info(
    On(server): On,
    axum::extract::Query(q): axum::extract::Query<api::DeviceCodeQuery>,
) -> Reply<api::DeviceRequest> {
    server
        .devices
        .device_info(&q.code)
        .map(Json)
        .ok_or_else(|| {
            fail(
                StatusCode::NOT_FOUND,
                "That code is not waiting for approval; start the sign-in again",
            )
        })
}

pub(super) async fn device_approve(
    On(server): On,
    Extension(who): Extension<Who>,
    Json(q): Json<api::DeviceApprove>,
) -> Reply<Done> {
    let message = if q.approve {
        "Approved; the command line is signed in"
    } else {
        "Denied"
    };
    done(
        server.devices.device_approve(&who, &q.code, q.approve),
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewers_read_the_terminal_view_and_members_review() {
        assert_eq!(needs(&Method::GET, "/api/v1/view"), Role::Viewer);
        assert_eq!(needs(&Method::POST, "/api/v1/review"), Role::Member);
        assert!(!Role::Viewer.at_least(needs(&Method::POST, "/api/v1/review")));
    }
}
