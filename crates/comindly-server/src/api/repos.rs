//! Repositories, indexing and jobs (`/api/v1/repos*`, `/index*`, `/jobs*`).

use serde::{Deserialize, Serialize};

pub use crate::events::Event;
pub use crate::hosting::{Provider, RemoteRepo};
pub use crate::jobs::{JobRecord, Status};

/// `GET /api/v1/repos`: the list `comindly link` indexes, and what the index has besides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repos {
    pub repos: Vec<RepoRow>,
    pub unlisted: Vec<UnlistedRepo>,
    /// The team's repositories with no folder here.
    pub team_missing: Vec<String>,
    pub root: Option<String>,
}

/// A listed repository and what the index holds of it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRow {
    pub path: String,
    pub name: String,
    pub exists: bool,
    pub branch: Option<String>,
    /// A repository comindly clones and keeps itself.
    pub managed: bool,
    pub cloned: bool,
    pub your_branch: Option<String>,
    pub remote: Option<String>,
    pub symbols: Option<usize>,
    pub indexed_commit: Option<String>,
}

/// A repository in the index with no folder in the list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlistedRepo {
    pub name: String,
    pub symbols: Option<usize>,
    pub commit: Option<String>,
    pub remote: Option<String>,
}

/// `POST /api/v1/repos/scan`: git repositories under a folder, `depth` 1 to 4 (2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanRequest {
    pub dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scan {
    pub ok: bool,
    pub found: Vec<ScanHit>,
}

/// A repository found under the folder, marked when already listed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanHit {
    pub name: String,
    pub listed: bool,
    pub remote: Option<String>,
    pub path: String,
}

/// `POST /api/v1/repos`: folders to add and remove.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoChange {
    #[serde(default)]
    pub add: Vec<String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// `POST /api/v1/repos/remote/list`: a GitHub organisation or GitLab group, with a token if needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteListRequest {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// The host's repositories, the ones already here marked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteList {
    pub ok: bool,
    pub provider: Provider,
    pub host: String,
    pub owner: String,
    pub repos: Vec<RemoteRow>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRow {
    #[serde(flatten)]
    pub repo: RemoteRepo,
    pub listed: bool,
}

/// `POST /api/v1/repos/remote/add`: clone URLs, optionally shared with the team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAddRequest {
    pub urls: Vec<String>,
    #[serde(default)]
    pub share: bool,
}

/// `POST /api/v1/index`: queue indexing of the listed repositories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexRequest {
    #[serde(default)]
    pub llm: bool,
    /// Index without the repositories that have no folder here, dropping them from the index.
    #[serde(default)]
    pub drop_unlisted: bool,
}

/// The job that runs it; `position` when it waits, 1 for the next to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStarted {
    pub ok: bool,
    pub message: String,
    pub id: String,
    pub status: Status,
    pub position: Option<usize>,
}

/// `GET /api/v1/index`: the latest job with its events, or none yet; and the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IndexState {
    Latest(LatestJob),
    Idle {
        running: bool,
        events: Vec<Event>,
        queued: Vec<QueuedJob>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatestJob {
    pub id: String,
    pub running: bool,
    pub cancelled: bool,
    /// Whether it succeeded, once it ended.
    pub ok: Option<bool>,
    pub status: Status,
    pub events: Vec<Event>,
    pub started: String,
    pub finished: Option<String>,
    pub queued: Vec<QueuedJob>,
}

/// A job waiting its turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedJob {
    pub id: String,
    pub kind: String,
    pub position: usize,
    pub started_by: Option<String>,
    pub created: String,
}

/// `GET /api/v1/index/watch`: whether automatic re-indexing is on and what it last found.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexWatch {
    pub enabled: bool,
    pub last: WatchResult,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatchResult {
    pub checked: Option<String>,
    pub result: String,
}

/// `GET /api/v1/jobs`: most recent first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jobs {
    pub jobs: Vec<JobRow>,
}

/// A job; a waiting one carries its `position`, 1 for the next to run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRow {
    #[serde(flatten)]
    pub job: JobRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<usize>,
}

/// `GET /api/v1/jobs/{id}`: the job, its events so far and its place in the queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDetail {
    pub job: JobRecord,
    pub events: Vec<Event>,
    pub position: Option<usize>,
}
