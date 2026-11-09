use serde::{Deserialize, Serialize};

/// Where the repositories live: a provider, its host and the owner or group path.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub provider: Provider,
    pub host: String,
    /// Organisation or user on GitHub; group or project path (`team/platform`) on GitLab.
    pub owner: String,
    /// The repository, when the URL names one (`github.com/acme/orders`, `….git`, `git@…`).
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    GitHub,
    GitLab,
}

/// A repository on the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRepo {
    pub name: String,
    /// `owner/name` or `group/sub/name`.
    pub full_path: String,
    pub clone_url: String,
    pub description: Option<String>,
    pub default_branch: Option<String>,
    pub archived: bool,
    pub fork: bool,
    pub last_activity: Option<String>,
    /// Size in KiB, when the host reports it.
    pub size_kib: Option<u64>,
}
