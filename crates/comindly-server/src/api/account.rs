//! Signing in, members, access tokens, secrets and the audit log (`/api/v1/auth/*`, `/members`,
//! `/tokens`, `/secrets`, `/audit`).

use serde::{Deserialize, Serialize};

pub use crate::server::{
    AccessToken, AuditEntry, AuditPage, AuditQuery, DeviceRequest, Invite, Member, Role,
    SecretInfo, SignIn,
};

/// `GET /api/v1/auth/me`: who is signed in, or `local` on this machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Me {
    Local,
    Server {
        signed_in: bool,
        email: Option<String>,
        role: Option<Role>,
        sign_in: SignIn,
        public_url: String,
    },
}

/// `{ email }`: request a sign-in link, reset a code, remove a member or withdraw an invitation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailRequest {
    pub email: String,
}

/// `POST /api/v1/auth/redeem`: a sign-in link's token, an invitation or the admin token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedeemRequest {
    pub token: String,
}

/// `POST /api/v1/auth/code`: sign in with an email address and a code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRequest {
    pub email: String,
    pub code: String,
}

/// A change that hands out a sign-in code: signing in with an invitation, or a reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WithCode {
    pub ok: bool,
    pub message: String,
    pub code: Option<String>,
}

/// `POST /api/v1/members/invite` and `/members/role`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberRole {
    pub email: String,
    pub role: Role,
}

/// An invitation sent: by email, or with a `code` the admin hands over.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invited {
    pub ok: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// `GET /api/v1/members`.
#[derive(Debug, Serialize, Deserialize)]
pub struct Members {
    pub members: Vec<Member>,
    pub invites: Vec<Invite>,
    pub sign_in: SignIn,
}

/// `GET /api/v1/tokens`: your personal access tokens.
#[derive(Debug, Serialize, Deserialize)]
pub struct Tokens {
    pub tokens: Vec<AccessToken>,
}

fn default_token_days() -> Option<u32> {
    Some(crate::server::TOKEN_DAYS)
}

/// `POST /api/v1/tokens`: absent `expires_in_days` means 90 days, `null` never.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewToken {
    pub name: String,
    #[serde(default = "default_token_days")]
    pub expires_in_days: Option<u32>,
}

/// A personal access token, shown once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenCreated {
    pub ok: bool,
    pub message: String,
    pub token: String,
}

/// `GET /api/v1/secrets`: where each token and key comes from, never its value.
#[derive(Debug, Serialize, Deserialize)]
pub struct Secrets {
    pub secrets: Vec<SecretState>,
    pub can_store: bool,
}

/// One secret: stored here with its details, or from the deployment or nowhere.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SecretState {
    Elsewhere(SecretSource),
    Stored {
        #[serde(flatten)]
        info: SecretInfo,
        label: String,
    },
}

/// A secret the deployment provides (`deployment`) or nobody set (`none`).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretSource {
    pub source: String,
    pub name: String,
    pub label: String,
}

/// `POST /api/v1/secrets`: an empty value removes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRequest {
    pub name: String,
    #[serde(default)]
    pub value: String,
}

/// `POST /api/v1/auth/device/start`: a command line asks to sign in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStartRequest {
    pub client: String,
}

/// Where to approve the sign-in, and how often to poll.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verify_url: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// `POST /api/v1/auth/device/token`: what the command line polls with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceTokenRequest {
    pub device_code: String,
}

/// Pending, or the token once a member approved; denied and expired are errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum DeviceToken {
    Pending,
    Approved {
        token: String,
        email: String,
        expires: Option<String>,
    },
}

/// `GET /api/v1/auth/device?code=`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeQuery {
    pub code: String,
}

/// `POST /api/v1/auth/device/approve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceApprove {
    pub code: String,
    pub approve: bool,
}
