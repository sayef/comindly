//! `~/.config/comindly/credentials`: your LLM API keys, a default and optionally one per team name,
//! and the server `comindly login` signed you in to. Readable only by you and never shared;
//! `OPENAI_API_KEY` is the fallback key.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    llm_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    team: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    server: Option<Login>,
}

/// A server and the personal access token `comindly login` received for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Login {
    pub url: String,
    pub token: String,
    pub email: String,
}

/// The server you are signed in to, if any.
pub fn login() -> Option<Login> {
    read().server
}

/// Remember the server you signed in to, or forget it with `None`.
pub fn set_login(login: Option<Login>) -> Result<()> {
    let mut f = read();
    f.server = login;
    write(&f)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    name: String,
    llm_api_key: String,
}

pub fn path() -> PathBuf {
    crate::config::config_dir().join("credentials")
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
    write(&f)
}

fn write(f: &File) -> Result<()> {
    let p = path();
    std::fs::create_dir_all(p.parent().context("config dir")?)?;
    let text = format!(
        "# comindly API keys and server sign-in. Keep this file private.\n\n{}",
        toml::to_string(f)?
    );
    let tmp = p.with_extension("tmp");
    let mut open = std::fs::OpenOptions::new();
    open.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
    std::io::Write::write_all(&mut open.open(&tmp)?, text.as_bytes())?;
    std::fs::rename(&tmp, &p).with_context(|| format!("write {}", p.display()))
}

#[cfg(test)]
mod tests {
    use crate::config::{export, Config};
    use comindly_core::pipeline::RepoEntry;

    // Written by toml 0.8; later versions must read and write the same files.
    const CREDENTIALS: &str = r#"llm_api_key = "k0"

[[team]]
name = "a"
llm_api_key = "k1"

[server]
url = "https://x.example"
token = "t"
email = "e@example.com"
"#;

    const PREFERENCES: &str = r#"# comindly preferences: `comindly config import <file> --override` applies this file.

index_dir = "s3://b/i"
signing_key = "~/.ssh/k"
cache_ttl_secs = 300
author_name = "A \"q\" B"
record_names = false

[[repos]]
path = "/r/a"
remote = "git@h:a.git"

[[repos]]
path = "/r/b"
"#;

    #[test]
    fn credentials_file_round_trips() {
        let f: super::File = toml::from_str(CREDENTIALS).unwrap();
        assert_eq!(f.team[0].name, "a");
        assert_eq!(f.server.as_ref().unwrap().email, "e@example.com");
        assert_eq!(toml::to_string(&f).unwrap(), CREDENTIALS);
    }

    #[test]
    fn preferences_file_round_trips() {
        let c: Config = toml::from_str(PREFERENCES).unwrap();
        assert_eq!(
            c,
            Config {
                index_dir: Some("s3://b/i".into()),
                signing_key: Some("~/.ssh/k".into()),
                allowed_signers: None,
                cache_ttl_secs: Some(300),
                author_name: Some("A \"q\" B".into()),
                record_names: Some(false),
                repos: vec![
                    RepoEntry {
                        path: "/r/a".into(),
                        remote: Some("git@h:a.git".into()),
                    },
                    RepoEntry {
                        path: "/r/b".into(),
                        remote: None,
                    },
                ],
            }
        );
        assert_eq!(export(&c).unwrap(), PREFERENCES);
    }
}
