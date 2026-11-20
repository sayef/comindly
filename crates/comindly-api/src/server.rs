use serde::{Deserialize, Serialize};

/// A way to sign in: an emailed link, a code, or an OpenID Connect provider; a server offers one or
/// more, each for invited people only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SignIn {
    Email,
    Code,
    Oidc,
}

/// What someone may do, each role including the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Members, repositories, settings, secrets and indexing.
    Admin,
    /// Writes to the ledger and conventions.
    Member,
    /// Reads.
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
            Role::Viewer => "viewer",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Role::Admin => 2,
            Role::Member => 1,
            Role::Viewer => 0,
        }
    }

    /// This role may do what `other` may.
    pub fn at_least(self, other: Role) -> bool {
        self.rank() >= other.rank()
    }
}

/// How a request proved who it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Session,
    AdminToken,
    /// A personal access token: enough for agents, not for managing the account.
    Token,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Member {
    pub email: String,
    pub role: Role,
    pub fixed: bool,
    pub created: String,
    pub last_seen: Option<String>,
    /// A service account: no person, no sign-in, only tokens an admin creates.
    #[serde(default)]
    pub service: bool,
    /// A service account's display name; its handle is in `email`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Service accounts are members at this reserved domain, which no mailbox or identity provider has.
pub const SERVICE_DOMAIN: &str = "service.invalid";

/// The member address of service account `name`: lowercase letters, digits and dashes, 1-40.
pub fn service_email(name: &str) -> Option<String> {
    let name = name.trim();
    let ok = (1..=40).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    ok.then(|| format!("{name}@{SERVICE_DOMAIN}"))
}

/// The handle a display name gives: "Build Bot 2" → `build-bot-2`.
pub fn service_handle(display: &str) -> String {
    let mut out = String::new();
    for c in display.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-')
        .chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

/// The service account's name, when `email` is one.
pub fn service_name(email: &str) -> Option<&str> {
    email
        .strip_suffix(SERVICE_DOMAIN)
        .and_then(|n| n.strip_suffix('@'))
        .filter(|n| !n.is_empty())
}

/// `POST /api/v1/members/service`: create a service account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewService {
    pub name: String,
    pub role: Role,
}

/// `POST /api/v1/members/service/token`: a token for a service account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTokenRequest {
    pub name: String,
    pub token_name: String,
    /// Days until it expires; `None` never.
    #[serde(default)]
    pub expires_in_days: Option<u32>,
    /// Addresses (`203.0.113.7`) or ranges (`203.0.113.0/24`) it works from; empty is anywhere.
    #[serde(default)]
    pub allowed: Vec<String>,
}

/// `POST /api/v1/members/tokens/revoke`: an admin revokes a member's token, or all with no `id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberTokenRevoke {
    pub email: String,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Invite {
    pub email: String,
    pub role: Role,
    pub created_by: String,
    pub expires: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessToken {
    pub id: String,
    pub name: String,
    pub created: String,
    pub last_used: Option<String>,
    /// `None` for a token that never expires.
    pub expires: Option<String>,
    pub expired: bool,
    /// Addresses or ranges it works from; empty is anywhere.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed: Vec<String>,
}

/// One row of the audit log; details never hold token values or codes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub at: String,
    pub actor: String,
    pub action: String,
    pub detail: String,
}

/// Which audit entries to list: `before` is the `next` cursor of the previous page.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AuditQuery {
    pub limit: Option<usize>,
    pub before: Option<String>,
    pub actor: Option<String>,
    pub action: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditPage {
    pub entries: Vec<AuditEntry>,
    /// The cursor for the next (older) page, when there is one.
    pub next: Option<String>,
}

/// A secret's state for the UI; its value never leaves the server.
#[derive(Debug, Serialize, Deserialize)]
pub struct SecretInfo {
    pub name: String,
    pub source: String,
    pub hint: Option<String>,
    pub updated: Option<String>,
    pub updated_by: Option<String>,
}

/// What a member sees before approving a command-line sign-in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRequest {
    pub client: String,
    pub expires: String,
}

/// Where a command-line sign-in stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevicePoll {
    Pending,
    Approved {
        token: String,
        email: String,
        expires: Option<String>,
    },
    Denied,
    Expired,
}

/// How long a token lasts unless chosen otherwise, and a command-line sign-in's always.
pub const TOKEN_DAYS: u32 = 90;

#[cfg(test)]
mod service_tests {
    use super::*;

    #[test]
    fn handles_come_from_display_names() {
        assert_eq!(service_handle("Build Bot 2"), "build-bot-2");
        assert_eq!(service_handle("  CI / nightly!! "), "ci-nightly");
        assert_eq!(service_handle("!!!"), "");
        assert_eq!(
            service_email("build-bot").as_deref(),
            Some("build-bot@service.invalid")
        );
        assert_eq!(service_name("build-bot@service.invalid"), Some("build-bot"));
        assert_eq!(service_name("ada@example.com"), None);
    }
}
