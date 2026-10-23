//! The requests and answers of `/api/v1`, `/health` and `/ready`: what the server writes and what
//! the CLI and the web app read. Grouped by area and re-exported here.

use serde::{Deserialize, Serialize};

mod account;
mod graph;
mod ledger;
mod repos;
mod settings;

pub use account::*;
pub use graph::*;
pub use ledger::*;
pub use repos::*;
pub use settings::*;

/// The read commands' answers (`/api/v1/answer/*`).
pub use crate::answers;

/// What most changes answer: done, and a line to show.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Done {
    pub ok: bool,
    pub message: String,
}

impl Done {
    pub fn new(message: impl Into<String>) -> Done {
        Done {
            ok: true,
            message: message.into(),
        }
    }
}

/// Every failed request: `{"error": {"code", "message"}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

/// Why a request failed; `host` and `username` when a clone needs an access token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// `GET /health`: the process answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub status: String,
    pub version: String,
}

/// `GET /ready`: an index is loaded and, on a server, the stores answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ready {
    pub ready: bool,
    pub index: bool,
    pub stores: bool,
}

/// `{ id }`: revoke a token, erase a record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdRequest {
    pub id: String,
}
