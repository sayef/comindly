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
