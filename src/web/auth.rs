//! Server mode in the web UI: sign-in by email link, invitations, members, personal access tokens
//! for agents, and the tokens and keys the server uses.

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::server::{self, Role, SignIn, Via, Who};

/// The session cookie; on HTTPS with the `__Host-` prefix, so no sibling domain can set it.
fn cookie_name() -> &'static str {
    if secure() {
        "__Host-comindly_session"
    } else {
        "comindly_session"
    }
}

fn secure() -> bool {
    server::public_url().is_some_and(|u| u.starts_with("https://"))
}

fn fail(status: StatusCode, message: impl Into<String>) -> Response {
    let message = message.into();
    (
        status,
        Json(json!({"ok": false, "message": message, "error": message})),
    )
        .into_response()
}

fn bad(e: anyhow::Error) -> Response {
    fail(StatusCode::BAD_REQUEST, super::error_text(&e))
}

fn unavailable() -> Response {
    fail(
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
fn same_site(req: &Request) -> bool {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|o| o.to_str().ok());
    match (origin, server::public_url()) {
        (Some(o), Some(url)) => origin_of(o).is_some_and(|o| Some(o) == origin_of(&url)),
        _ => req
            .headers()
            .get("sec-fetch-site")
            .is_some_and(|v| v == "same-origin"),
    }
}

/// What a personal access token may not do: manage the account, its tokens, members or secrets.
fn account_only(path: &str) -> bool {
    ["/api/auth/", "/api/tokens", "/api/members", "/api/secrets"]
        .iter()
        .any(|p| path.starts_with(p))
}

/// Paths only admins may change (and, for members and secrets, read).
fn admin_only(method: &Method, path: &str) -> bool {
    if path.starts_with("/api/members") || path.starts_with("/api/secrets") {
        return true;
    }
    method != Method::GET
        && [
            "/api/settings",
            "/api/repos",
            "/api/index",
            "/api/team",
            "/api/join",
            "/api/erase",
        ]
        .iter()
        .any(|p| path.starts_with(p))
}

/// Who may do what in server mode: the session cookie or a bearer token decides.
pub(super) async fn guard(mut req: Request, next: Next) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    let path = req.uri().path().to_string();
    if !(path.starts_with("/api/") || path.starts_with("/mcp")) {
        return next.run(req).await;
    }
    let open = matches!(
        path.as_str(),
        "/api/auth/me" | "/api/auth/link" | "/api/auth/redeem" | "/api/auth/code"
    );
    let token = bearer(&req);
    let session = cookie(&req, cookie_name());
    let found = tokio::task::block_in_place(|| {
        store
            .authenticate(session.as_deref(), token.as_deref())
            .unwrap_or(None)
    });
    if let Some((w, _)) = &found {
        req.extensions_mut().insert(w.clone());
    }
    if open {
        return next.run(req).await;
    }
    let Some((who, via)) = found else {
        return fail(StatusCode::UNAUTHORIZED, "Sign in to continue");
    };
    if via == Via::Token && account_only(&path) {
        return fail(
            StatusCode::FORBIDDEN,
            "Personal access tokens are for agents; sign in to manage your account",
        );
    }
    if via == Via::Session && req.method() != Method::GET && !same_site(&req) {
        return fail(
            StatusCode::FORBIDDEN,
            "This request did not come from comindly's own pages",
        );
    }
    if admin_only(req.method(), &path) && who.role != Role::Admin {
        return fail(StatusCode::FORBIDDEN, "Only admins can do this");
    }
    next.run(req).await
}

/// The names this server answers to: its public address, and localhost for checks on the host.
pub(super) fn hosts() -> Vec<String> {
    let mut out: Vec<String> = ["localhost", "127.0.0.1", "::1"].map(String::from).to_vec();
    if let Some(url) = server::public_url() {
        let authority = url.split("://").nth(1).unwrap_or(&url);
        let authority = authority.split('/').next().unwrap_or(authority);
        out.push(authority.to_string());
        out.push(authority.split(':').next().unwrap_or(authority).to_string());
    }
    out
}

pub(super) async fn me(who: Option<Extension<Who>>) -> Json<Value> {
    if !server::enabled() {
        return Json(json!({"mode": "local"}));
    }
    Json(json!({
        "mode": "server",
        "signed_in": who.is_some(),
        "email": who.as_ref().map(|w| w.email.clone()),
        "role": who.as_ref().map(|w| w.role),
        "sign_in": server::mode(),
        "public_url": server::public_url(),
    }))
}

fn link(path: &str, token: &str) -> String {
    format!(
        "{}/{path}?t={token}",
        server::public_url().unwrap_or_default()
    )
}

#[derive(Deserialize)]
pub(super) struct EmailQ {
    email: String,
}

/// Email a sign-in link; the answer is the same whether or not the address is a member.
pub(super) async fn request_link(Json(q): Json<EmailQ>) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    if server::mode() != SignIn::Email {
        return fail(StatusCode::BAD_REQUEST, "This server signs in with codes");
    }
    let email = server::normalize(&q.email);
    // Sent in the background, so the answer neither waits on the mail server nor reveals a member.
    match store.sign_in_link(&email) {
        Ok(Some(token)) => {
            let body = format!(
                "Sign in to comindly with this link (valid for 15 minutes, once):\n\n{}\n\nIf you did not ask for it, ignore this email.",
                link("signin", &token)
            );
            tokio::spawn(async move {
                if let Err(e) = server::send_mail(&email, "Your comindly sign-in link", &body).await
                {
                    crate::ui::warn(&super::error_text(&e));
                }
            });
        }
        Ok(None) => {}
        Err(e) => crate::ui::warn(&format!("sign-in link: {}", super::error_text(&e))),
    }
    Json(json!({"ok": true, "message": "If that address belongs to a member, a sign-in link is on its way"}))
        .into_response()
}

#[derive(Deserialize)]
pub(super) struct TokenQ {
    token: String,
}

fn session_cookie(token: &str, max_age: i64) -> String {
    format!(
        "{}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        cookie_name(),
        if secure() { "; Secure" } else { "" }
    )
}

/// Exchange a sign-in link, an invitation or the admin token for a session cookie.
pub(super) async fn redeem(
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(q): Json<TokenQ>,
) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    match store.redeem(&q.token, &client(peer, &headers)) {
        Ok((session, who)) => (
            StatusCode::OK,
            [(header::SET_COOKIE, session_cookie(&session, 30 * 86400))],
            Json(json!({"ok": true, "message": format!("Signed in as {}", who.email)})),
        )
            .into_response(),
        Err(e) => bad(e),
    }
}

#[derive(Deserialize)]
pub(super) struct CodeQ {
    email: String,
    code: String,
}

/// Where a request comes from: the connecting address, or behind a proxy you run
/// (`COMINDLY_TRUST_PROXY=true`) the address that proxy added last to `X-Forwarded-For`.
fn client(peer: std::net::SocketAddr, headers: &axum::http::HeaderMap) -> String {
    let forwarded = server::env_secret("COMINDLY_TRUST_PROXY")
        .is_some_and(|v| v == "true" || v == "1")
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
    axum::extract::ConnectInfo(peer): axum::extract::ConnectInfo<std::net::SocketAddr>,
    headers: axum::http::HeaderMap,
    Json(q): Json<CodeQ>,
) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    if server::mode() != SignIn::Code {
        return fail(StatusCode::BAD_REQUEST, "This server signs in by email");
    }
    match store.sign_in_code(&q.email, &q.code, &client(peer, &headers)) {
        Ok((session, who, code)) => (
            StatusCode::OK,
            [(header::SET_COOKIE, session_cookie(&session, 30 * 86400))],
            Json(
                json!({"ok": true, "message": format!("Signed in as {}", who.email), "code": code}),
            ),
        )
            .into_response(),
        Err(e) => bad(e),
    }
}

/// A new sign-in code for yourself; your other sessions end.
pub(super) async fn reset_own_code(Extension(who): Extension<Who>, req: Request) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    if server::mode() != SignIn::Code {
        return fail(StatusCode::BAD_REQUEST, "This server signs in by email");
    }
    match store.reset_code(&who.email, &who.email, cookie(&req, cookie_name()).as_deref()) {
        Ok(code) => Json(json!({"ok": true, "message": "New sign-in code; the old one no longer works", "code": code}))
            .into_response(),
        Err(e) => bad(e),
    }
}

pub(super) async fn sign_out(req: Request) -> Response {
    let everywhere = req
        .uri()
        .query()
        .is_some_and(|q| q.contains("everywhere=true"));
    if let (Some(store), Some(s)) = (server::store(), cookie(&req, cookie_name())) {
        let _ = store.sign_out(&s, everywhere);
    }
    (
        StatusCode::OK,
        [(header::SET_COOKIE, session_cookie("", 0))],
        Json(json!({"ok": true, "message": "Signed out"})),
    )
        .into_response()
}

pub(super) async fn members() -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    match (store.members(), store.invites()) {
        (Ok(m), Ok(i)) => {
            Json(json!({"members": m, "invites": i, "sign_in": server::mode()})).into_response()
        }
        (Err(e), _) | (_, Err(e)) => bad(e),
    }
}

#[derive(Deserialize)]
pub(super) struct InviteQ {
    email: String,
    role: Role,
}

/// Invite `email`: by email, or with a code the admin hands over (returned once).
pub(super) async fn invite(Extension(who): Extension<Who>, Json(q): Json<InviteQ>) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    let email = server::normalize(&q.email);
    let code = server::mode() == SignIn::Code;
    let token = match store.invite(&who.email, &email, q.role, code) {
        Ok(t) => t,
        Err(e) => return bad(e),
    };
    if code {
        return Json(json!({"ok": true, "message": format!("Invited {email}; give them this code"), "code": token}))
            .into_response();
    }
    let body = format!(
        "{} invited you to comindly. Join with this link (valid for 7 days, once, for {email} only):\n\n{}",
        who.email,
        link("signin", &token)
    );
    match server::send_mail(&email, "You're invited to comindly", &body).await {
        Ok(()) => Json(json!({"ok": true, "message": format!("Invitation emailed to {email}")}))
            .into_response(),
        Err(e) => {
            let _ = store.revoke_invite(&who.email, &email);
            bad(e)
        }
    }
}

/// A new sign-in code for a member who lost theirs; their sessions end.
pub(super) async fn reset_code(Extension(who): Extension<Who>, Json(q): Json<EmailQ>) -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
    if server::mode() != SignIn::Code {
        return fail(StatusCode::BAD_REQUEST, "This server signs in by email");
    }
    match store.reset_code(&who.email, &q.email, None) {
        Ok(code) => {
            Json(json!({"ok": true, "message": "New sign-in code; give it to them", "code": code}))
                .into_response()
        }
        Err(e) => bad(e),
    }
}

#[derive(Deserialize)]
pub(super) struct RoleQ {
    email: String,
    role: Role,
}

pub(super) async fn set_role(Extension(who): Extension<Who>, Json(q): Json<RoleQ>) -> Response {
    done(
        server::store().map(|s| s.set_role(&who.email, &q.email, q.role)),
        "Role changed",
    )
}

pub(super) async fn remove(Extension(who): Extension<Who>, Json(q): Json<EmailQ>) -> Response {
    if server::normalize(&q.email) == who.email {
        return fail(StatusCode::BAD_REQUEST, "You cannot remove yourself");
    }
    done(
        server::store().map(|s| s.remove(&who.email, &q.email)),
        "Member removed",
    )
}

pub(super) async fn revoke_invite(
    Extension(who): Extension<Who>,
    Json(q): Json<EmailQ>,
) -> Response {
    done(
        server::store().map(|s| s.revoke_invite(&who.email, &q.email)),
        "Invitation withdrawn",
    )
}

fn done(r: Option<anyhow::Result<()>>, message: &str) -> Response {
    match r {
        Some(Ok(())) => Json(json!({"ok": true, "message": message})).into_response(),
        Some(Err(e)) => bad(e),
        None => unavailable(),
    }
}

pub(super) async fn tokens(Extension(who): Extension<Who>) -> Response {
    match server::store().map(|s| s.tokens(&who)) {
        Some(Ok(t)) => Json(json!({"tokens": t})).into_response(),
        Some(Err(e)) => bad(e),
        None => unavailable(),
    }
}

#[derive(Deserialize)]
pub(super) struct NameQ {
    name: String,
}

/// A personal access token for an agent; shown once.
pub(super) async fn create_token(Extension(who): Extension<Who>, Json(q): Json<NameQ>) -> Response {
    match server::store().map(|s| s.create_token(&who, &q.name)) {
        Some(Ok(t)) => Json(json!({"ok": true, "message": "Token created; copy it now, it is shown once", "token": t}))
            .into_response(),
        Some(Err(e)) => bad(e),
        None => unavailable(),
    }
}

#[derive(Deserialize)]
pub(super) struct IdQ {
    id: String,
}

pub(super) async fn revoke_token(Extension(who): Extension<Who>, Json(q): Json<IdQ>) -> Response {
    done(
        server::store().map(|s| s.revoke_token(&who, &q.id)),
        "Token revoked",
    )
}

/// The deployment's value for a secret, which always wins over one stored here.
fn deployment(name: &str) -> Option<String> {
    if name == server::LLM_SECRET {
        return server::env_secret("OPENAI_API_KEY");
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
pub(super) async fn secrets() -> Response {
    let Some(store) = server::store() else {
        return unavailable();
    };
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
    let list: Vec<Value> = names
        .iter()
        .map(|n| {
            let info = if deployment(n).is_some() {
                json!({"source": "deployment"})
            } else if let Some(i) = store.secret_info(n) {
                json!(i)
            } else {
                json!({"source": "none"})
            };
            let mut v = info;
            v["name"] = json!(n);
            v["label"] = json!(label(n));
            v
        })
        .collect();
    Json(json!({
        "secrets": list,
        "can_store": server::env_secret("COMINDLY_SECRET_KEY").is_some(),
    }))
    .into_response()
}

#[derive(Deserialize)]
pub(super) struct SecretQ {
    name: String,
    #[serde(default)]
    value: String,
}

/// Store or remove a token or key; one the deployment provides cannot be changed here.
pub(super) async fn set_secret(Extension(who): Extension<Who>, Json(q): Json<SecretQ>) -> Response {
    let name = q.name.trim().to_lowercase();
    let valid = name == server::LLM_SECRET
        || name.strip_prefix("git:").is_some_and(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c))
        });
    if !valid {
        return fail(StatusCode::BAD_REQUEST, "Unknown secret");
    }
    if deployment(&name).is_some() {
        return fail(
            StatusCode::BAD_REQUEST,
            "The deployment provides this one; change it where the deployment keeps its secrets",
        );
    }
    let message = if q.value.trim().is_empty() {
        "Removed"
    } else {
        "Saved"
    };
    done(
        server::store().map(|s| s.set_secret(&who.email, &name, &q.value)),
        message,
    )
}
