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

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads `json` as `T` and writes it back unchanged, keys in the same order.
    fn same<T: Serialize + serde::de::DeserializeOwned>(json: &str) {
        let v: T = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    #[test]
    fn answers_keep_their_wire_shape() {
        same::<Me>(r#"{"mode":"local"}"#);
        same::<Me>(
            r#"{"mode":"server","signed_in":false,"email":null,"role":null,"sign_in":"code","public_url":"http://localhost:1"}"#,
        );
        same::<DeviceToken>(r#"{"status":"pending"}"#);
        same::<DeviceToken>(
            r#"{"status":"approved","token":"t","email":"a@example.com","expires":null}"#,
        );
        same::<Secrets>(
            r#"{"secrets":[{"source":"deployment","name":"llm_api_key","label":"LLM API key"},{"name":"git:github.com","source":"stored","hint":"…abcd","updated":"2026-01-01T00:00:00+00:00","updated_by":"a@example.com","label":"GitHub token for github.com"}],"can_store":true}"#,
        );
        same::<IndexState>(r#"{"running":false,"events":[],"queued":[]}"#);
        same::<IndexState>(
            r#"{"id":"j","running":false,"cancelled":false,"ok":true,"status":"succeeded","events":[],"started":"s","finished":"f","queued":[{"id":"k","kind":"index","position":1,"started_by":null,"created":"c"}]}"#,
        );
        same::<Jobs>(
            r#"{"jobs":[{"id":"k","kind":"index","started_by":null,"status":"queued","created":"c","started":null,"finished":null,"error":null,"position":1}]}"#,
        );
        same::<Invited>(r#"{"ok":true,"message":"Invitation emailed to a@example.com"}"#);
        same::<ErrorBody>(r#"{"error":{"code":"not_found","message":"no such job"}}"#);
        same::<Neighbourhood>(r#"{"nodes":[],"edges":[]}"#);
    }
}
