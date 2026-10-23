//! What a server stores and how it signs people in, as traits; `server::Store` and [`Smtp`] are the
//! defaults.

use anyhow::{Context, Result};
use futures::future::BoxFuture;

use crate::server::{
    AccessToken, DevicePoll, DeviceRequest, Invite, Member, Role, SecretInfo, Via, Who,
};

/// Members, invitations, sign-in links and codes, sessions and personal access tokens.
pub trait AccountStore: Send + Sync {
    /// The member `email`; deployment admins are always present and admin.
    fn member(&self, email: &str) -> Result<Option<Who>>;
    fn members(&self) -> Result<Vec<Member>>;
    fn set_role(&self, by: &str, email: &str, role: Role) -> Result<()>;
    /// Remove a member with their sessions, tokens and invitations.
    fn remove(&self, by: &str, email: &str) -> Result<()>;
    /// Invite `email`; returns the link token, or with `code` the invitation code.
    fn invite(&self, by: &str, email: &str, role: Role, code: bool) -> Result<String>;
    fn invites(&self) -> Result<Vec<Invite>>;
    fn revoke_invite(&self, by: &str, email: &str) -> Result<()>;
    /// A sign-in link token for a member; `None` for anyone else.
    fn sign_in_link(&self, email: &str) -> Result<Option<String>>;
    /// Exchange a link, an invitation or the admin token for a session.
    fn redeem(&self, token: &str, client: &str) -> Result<(String, Who)>;
    /// Sign in with an email and a code; an invitation code also returns the person's own code.
    fn sign_in_code(
        &self,
        email: &str,
        code: &str,
        client: &str,
    ) -> Result<(String, Who, Option<String>)>;
    /// A new sign-in code for `email`; their sessions end, except `keep`.
    fn reset_code(&self, by: &str, email: &str, keep: Option<&str>) -> Result<String>;
    fn sign_out(&self, session: &str, everywhere: bool) -> Result<()>;
    fn create_token(&self, who: &Who, name: &str) -> Result<String>;
    fn tokens(&self, who: &Who) -> Result<Vec<AccessToken>>;
    fn revoke_token(&self, who: &Who, id: &str) -> Result<()>;
}

/// Command-line sign-in approved in the browser: the command starts it and polls, a signed-in
/// member approves it, and the command receives a personal access token.
pub trait DeviceSignIn: Send + Sync {
    /// A device code for the command and a short code for the member to confirm.
    fn device_start(&self, client: &str, from: &str) -> Result<(String, String)>;
    fn device_info(&self, user_code: &str) -> Option<DeviceRequest>;
    fn device_approve(&self, who: &Who, user_code: &str, approve: bool) -> Result<()>;
    fn device_poll(&self, device_code: &str) -> Result<DevicePoll>;
}

/// Who a request comes from: a session cookie or a bearer token, or nobody.
pub trait Authenticator: Send + Sync {
    fn authenticate(
        &self,
        session: Option<&str>,
        bearer: Option<&str>,
    ) -> Result<Option<(Who, Via)>>;
}

/// Git and LLM tokens, by name.
pub trait SecretStore: Send + Sync {
    /// Store `value` under `name`; an empty value removes it.
    fn set_secret(&self, by: &str, name: &str, value: &str) -> Result<()>;
    fn secret(&self, name: &str) -> Option<String>;
    fn secret_info(&self, name: &str) -> Option<SecretInfo>;
    fn secret_names(&self) -> Vec<String>;
}

/// The server's `config.toml`, kept so a new container starts with it.
pub trait SettingsStore: Send + Sync {
    fn keep_config(&self, text: &str) -> Result<()>;
    fn kept_config(&self) -> Option<String>;
}

/// Sends sign-in links and invitations.
pub trait Mailer: Send + Sync {
    fn send<'a>(
        &'a self,
        to: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<()>>;
}

/// Mail through `COMINDLY_SMTP_URL` (e.g. `smtps://user:pass@host`).
pub struct Smtp {
    pub url: Option<String>,
    pub from: String,
}

impl Mailer for Smtp {
    fn send<'a>(
        &'a self,
        to: &'a str,
        subject: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            use lettre::message::header::ContentType;
            use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
            let url = self.url.as_deref().context("no mail server configured")?;
            let msg = Message::builder()
                .from(
                    self.from
                        .parse()
                        .context("COMINDLY_SMTP_FROM is not an address")?,
                )
                .to(to.parse().context("not an email address")?)
                .subject(subject)
                .header(ContentType::TEXT_PLAIN)
                .body(body.to_string())?;
            AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
                .context("COMINDLY_SMTP_URL is not a valid smtp:// or smtps:// URL")?
                .build()
                .send(msg)
                .await
                .map_err(|e| anyhow::anyhow!("sending mail failed: {e}"))?;
            Ok(())
        })
    }
}
