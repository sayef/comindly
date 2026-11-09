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

/// `POST /api/v1/teams`: the whole team list, at the settings version read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamsSave {
    pub teams: Vec<crate::team::TeamDef>,
    pub version: Option<u64>,
}

/// `POST /api/v1/repos/teams`: exactly these teams on `repo` (its indexed name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoTeams {
    pub repo: String,
    pub teams: Vec<String>,
}

/// `POST /api/v1/me/teams`: the teams you belong to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyTeams {
    pub teams: Vec<String>,
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

/// `GET /api/v1/prompts`: each LLM task's built-in prompt, its saved versions and the one in use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prompts {
    pub tasks: Vec<PromptTask>,
    pub version: Option<u64>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTask {
    pub task: crate::prompts::Task,
    pub builtin: String,
    /// The reply format and its JSON schema, added after whichever instructions run.
    pub format: String,
    /// `None`: the built-in prompt.
    pub active: Option<u32>,
    /// The active version, when a release changed the built-in prompt since it was written.
    pub builtin_changed: Option<u32>,
    /// Style guide only: the built-in guidance of each section.
    pub sections: Vec<GuideSectionText>,
    pub versions: Vec<crate::prompts::Version>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideSectionText {
    pub title: String,
    pub guidance: String,
}

/// `POST /api/v1/prompts`: one change, at the version read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptChange {
    pub version: Option<u64>,
    #[serde(flatten)]
    pub action: PromptAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum PromptAction {
    /// A new version, not in use until activated.
    Save {
        task: crate::prompts::Task,
        text: String,
        /// Style guide only: section guidance by title.
        #[serde(default)]
        sections: std::collections::BTreeMap<String, String>,
        #[serde(default)]
        note: Option<String>,
    },
    Activate {
        task: crate::prompts::Task,
        n: u32,
    },
    /// Back to the built-in prompt.
    Deactivate {
        task: crate::prompts::Task,
    },
    Delete {
        task: crate::prompts::Task,
        n: u32,
    },
}

/// `POST /api/v1/prompts/try`: run `text` on one sample without saving it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTry {
    pub task: crate::prompts::Task,
    pub text: String,
    #[serde(default)]
    pub sections: std::collections::BTreeMap<String, String>,
    /// A symbol id for a summary, an entry point for a walkthrough, a repository for a style
    /// guide; otherwise the most used one, the furthest-reaching flow or the first repository.
    #[serde(default)]
    pub sample: Option<String>,
    /// Style guide only: the section to write; the first one otherwise.
    #[serde(default)]
    pub section: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTried {
    pub sample: String,
    pub output: String,
    /// What the index holds for the same sample now.
    pub current: Option<String>,
}

/// `GET`/`POST /api/v1/logging`: which kinds of lines the engine logs, switched without a restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Logging {
    /// One line per request.
    pub access: bool,
    /// One line per MCP tool call, with the client's name.
    pub tool: bool,
}
