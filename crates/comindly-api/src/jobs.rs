use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    /// The process stopped while it ran; what was saved stays, and it can run again.
    Interrupted,
    /// Stopped by a restart; it continues by itself once a server is back.
    Paused,
}

impl Status {
    pub fn finished(self) -> bool {
        !matches!(self, Status::Queued | Status::Running | Status::Paused)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub started_by: Option<String>,
    pub status: Status,
    pub created: String,
    #[serde(default)]
    pub started: Option<String>,
    pub finished: Option<String>,
    pub error: Option<String>,
    /// The request that started it, as in its `X-Request-Id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// What to run again after a restart; a job without it cannot continue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<serde_json::Value>,
    /// The running job's last sign of life, so a starting server leaves a live one alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat: Option<String>,
}
