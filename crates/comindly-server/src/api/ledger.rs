//! The ledger and conventions (`/api/v1/ledger`, `/remember`, `/review`, `/erase`,
//! `/conventions`), and the queries of the read commands' answers (`/api/v1/answer/*`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::conventions::{Convention, Manual};
pub use crate::ledger::Entry;
pub use crate::preflight::Change;

/// `GET /api/v1/conventions?repo=`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// The conventions in effect, and the measured ones a person hid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conventions {
    pub conventions: Vec<ShownConvention>,
    pub hidden: Vec<Manual>,
}

/// A convention with the handle edits use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShownConvention {
    #[serde(flatten)]
    pub convention: Convention,
    pub id: String,
}

/// `POST /api/v1/conventions`: `add`, `edit`, `hide` or `restore`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConventionChange {
    pub action: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rule: String,
}

/// `POST /api/v1/review`: `confirm`, `archive`, `invalidate`, … a record by id or id prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub id: String,
    pub action: String,
    #[serde(default)]
    pub reason: String,
}

/// `POST /api/v1/remember`: a record written by a person.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberRequest {
    pub kind: String,
    /// Symbols the record is about: ids, names or `Class.method`.
    pub on: Vec<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

/// The new record's id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remembered {
    pub ok: bool,
    pub id: String,
}

/// `answer/find` and `answer/search`: `repos` is a comma-separated list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerQuery {
    pub q: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<String>,
}

/// `answer/why`, `answer/flow` and `answer/explore`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerFocus {
    pub focus: String,
}

/// `answer/ledger`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerQuery {
    #[serde(default)]
    pub inbox: bool,
    #[serde(default)]
    pub check: bool,
}

/// `answer/contracts`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContractsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default)]
    pub dangling: bool,
}

/// `answer/stats`: `repos` is a comma-separated list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatsQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<String>,
}

/// `answer/packages`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackagesQuery {
    #[serde(default)]
    pub skew: bool,
}

/// `POST answer/preflight`: the changed lines, read from git by the caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightRequest {
    pub changes: Vec<Change>,
}
