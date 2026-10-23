//! Your settings and the team's (`/api/v1/settings*`, `/team*`, `/join`).

use serde::{Deserialize, Serialize};

pub use crate::credentials::Source as KeySource;
pub use crate::team::TeamSettings;

/// `GET /api/v1/settings`: your preferences and what is in effect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub config_path: String,
    pub index_dir: Option<String>,
    pub index_effective: String,
    pub author_name: Option<String>,
    pub author_effective: Option<String>,
    pub record_names: bool,
    pub llm_model: String,
    pub defaults: Defaults,
    pub llm_key: KeySource,
    pub cache_ttl_secs: u64,
}

/// comindly's built-in values, shown where a setting is left empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    pub llm_model: String,
    pub llm_base_url: String,
    pub embed_model: String,
    pub llm_concurrency: usize,
    pub format: String,
}

/// `POST /api/v1/settings`: only the given settings change; an empty text removes one.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SettingsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_names: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_secs: Option<u64>,
}

/// `POST /api/v1/settings/test`: whether an index location can be read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDirRequest {
    pub index_dir: String,
}

/// `GET /api/v1/team`: the index's shared settings and where your API key comes from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub settings: TeamSettings,
    pub key_source: KeySource,
    pub version: Option<u64>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
    pub defaults: Defaults,
}

/// `POST /api/v1/team`: `version` is the one read, so a concurrent change is not overwritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamSave {
    pub settings: TeamSettings,
    pub version: Option<u64>,
}

/// `POST /api/v1/team/key`: your API key; empty removes it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyRequest {
    pub key: String,
}

/// `POST /api/v1/join`: use a team's index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub index: String,
}

/// `POST /api/v1/team/create`: start a team at an empty location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTeam {
    pub name: String,
    pub index: String,
}
