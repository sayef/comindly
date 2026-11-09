use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A new step begins; its message names it.
    Step,
    /// A result.
    Done,
    /// A detail.
    Note,
    Warning,
    Error,
    /// How far a step is: `done` of `total`, with LLM usage so far.
    Progress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub timestamp: String,
    pub level: Level,
    pub kind: Kind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// The job this event belongs to, when it runs as one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_in: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_out: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
}

impl Event {
    pub fn new(level: Level, kind: Kind, message: impl Into<String>) -> Self {
        Event {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level,
            kind,
            message: message.into(),
            step: None,
            job_id: None,
            done: None,
            total: None,
            tokens_in: None,
            tokens_out: None,
            usd: None,
        }
    }
}
