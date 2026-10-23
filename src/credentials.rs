//! `~/.config/comindly/credentials`: your LLM API keys, a default and optionally one per team name.
//! Created readable only by you and never shared; `OPENAI_API_KEY` is the fallback.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    llm_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    team: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    name: String,
    llm_api_key: String,
}

pub fn path() -> PathBuf {
    crate::config::config_path()
        .parent()
        .map(|d| d.join("credentials"))
        .unwrap_or_else(|| PathBuf::from("credentials"))
}

fn read() -> File {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default()
}

/// Where an API key comes from, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Team,
    Default,
    Environment,
    Server,
    None,
}

/// The API key for `team` (a team name): its entry, else the default, else `OPENAI_API_KEY`;
/// on a server, the deployment's key or the one stored there.
pub fn llm_key(
    team: Option<&str>,
    server: Option<&crate::server::Server>,
) -> (Option<String>, Source) {
    if let Some(server) = server {
        if let Some(k) = server.config.llm_key.clone() {
            return (Some(k), Source::Environment);
        }
        return match server.secrets.secret(crate::server::LLM_SECRET) {
            Some(k) => (Some(k), Source::Server),
            None => (None, Source::None),
        };
    }
    let f = read();
    if let Some(k) = team.and_then(|t| f.team.iter().find(|e| e.name == t)) {
        return (Some(k.llm_api_key.clone()), Source::Team);
    }
    if let Some(k) = f.llm_api_key.filter(|k| !k.is_empty()) {
        return (Some(k), Source::Default);
    }
    match std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
    {
        Some(k) => (Some(k), Source::Environment),
        None => (None, Source::None),
    }
}

/// Store `key` for `team`, or as the default without one; an empty key removes it.
pub fn set_llm_key(team: Option<&str>, key: &str) -> Result<()> {
    let mut f = read();
    let key = key.trim();
    match team {
        Some(t) => {
            f.team.retain(|e| e.name != t);
            if !key.is_empty() {
                f.team.push(Entry {
                    name: t.to_string(),
                    llm_api_key: key.to_string(),
                });
            }
        }
        None => f.llm_api_key = (!key.is_empty()).then(|| key.to_string()),
    }
    let p = path();
    std::fs::create_dir_all(p.parent().context("config dir")?)?;
    let text = format!(
        "# comindly API keys: a default, and one per team name if needed. Keep this file private.\n\n{}",
        toml::to_string(&f)?
    );
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, &p).with_context(|| format!("write {}", p.display()))
}
