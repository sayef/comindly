//! Your preferences on this machine — which index to open, the repositories this machine indexes,
//! and how your ledger records are named and signed — kept in a versioned LanceDB table in
//! comindly's config folder. A TOML file of the same shape can be imported and exported. What an
//! index runs with is stored with the index; a server keeps its settings in its database.
//!
//! Precedence, highest first: CLI flag → environment variable → these preferences → default.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Non-secret preferences, all optional.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Config {
    /// Default index directory for `--index-dir` (local path or `s3://…`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_dir: Option<String>,
    /// SSH key that signs ledger records (default: git `user.signingkey` when `gpg.format=ssh`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing_key: Option<String>,
    /// `allowed_signers` file for verifying records (default: git `gpg.ssh.allowedSignersFile`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_signers: Option<String>,
    /// Seconds between freshness checks of the local mirror of a remote index (default 300).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_ttl_secs: Option<u64>,
    /// Name shown on ledger records you write (default: git `user.name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// `false` keeps names off new ledger records (only the pseudonymous actor id is stored).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_names: Option<bool>,
    /// Repositories `comindly link` indexes when given none: comindly checks out each one's default
    /// branch in its own worktree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<RepoEntry>,
}

/// The preferences, by the names files and `comindly config set` use.
pub const KEYS: [&str; 7] = [
    "index_dir",
    "signing_key",
    "allowed_signers",
    "cache_ttl_secs",
    "author_name",
    "record_names",
    "repos",
];

pub use crate::pipeline::RepoEntry;

/// `repos` with `~` in their paths expanded.
pub fn expanded(repos: &[RepoEntry]) -> Vec<RepoEntry> {
    repos
        .iter()
        .map(|r| RepoEntry {
            path: expand_tilde(&r.path),
            remote: r.remote.clone(),
        })
        .collect()
}

const TABLE: &str = "preferences";

/// Where the preferences are kept: `<config folder>/settings`.
pub fn store_path() -> PathBuf {
    config_dir().join("settings")
}

/// The stored preferences with their version.
pub fn stored() -> Result<crate::docstore::Stored<Config>> {
    let uri = store_path().to_string_lossy().into_owned();
    crate::rt::block_on(crate::docstore::load(&uri, TABLE))
}

fn save(cfg: &Config, expected: Option<u64>) -> Result<u64> {
    let uri = store_path().to_string_lossy().into_owned();
    let me = local_identity();
    let by = me.name.unwrap_or(me.actor);
    crate::rt::block_on(crate::docstore::save(&uri, TABLE, cfg, expected, &by))
}

/// Every saved version of the preferences, newest first.
pub fn history() -> Result<Vec<crate::docstore::Version>> {
    let uri = store_path().to_string_lossy().into_owned();
    crate::rt::block_on(crate::docstore::history(&uri, TABLE))
}

/// Replace the repository list.
pub fn set_repos(list: &[RepoEntry]) -> Result<()> {
    let s = stored()?;
    let mut cfg = s.doc;
    cfg.repos = list.to_vec();
    save(&cfg, s.version).map(drop)
}

impl Config {
    /// The stored preferences; on a read error, a warning and the defaults.
    pub fn load() -> Self {
        match stored() {
            Ok(s) => s.doc,
            Err(e) => {
                crate::ui::warn(&format!("your preferences could not be read: {e:#}"));
                Self::default()
            }
        }
    }

    /// Resolve the index location: flag → `COMINDLY_INDEX_DIR` → preferences → `~/.local/share/comindly`.
    pub fn index_dir(&self, flag: Option<&str>) -> String {
        if let Some(f) = flag {
            return expand_tilde(f);
        }
        if let Some(e) = non_empty_env("COMINDLY_INDEX_DIR") {
            return expand_tilde(&e);
        }
        if let Some(d) = &self.index_dir {
            return expand_tilde(d);
        }
        default_index_dir()
    }

    /// Resolve the graph dataset to read from: `<index_dir>/_graph`. The `_graph` suffix is an
    /// internal detail — callers pass the index dir (root), never the dataset path.
    pub fn graph_dir(&self, flag: Option<&str>) -> String {
        format!("{}/_graph", self.index_dir(flag).trim_end_matches('/'))
    }

    /// How often long-running readers (`serve`, `ui`, the hooks' local mirror) check for a new
    /// index version.
    pub fn fresh(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.cache_ttl_secs.unwrap_or(crate::cache::DEFAULT_TTL))
    }
}

/// Where older versions kept preferences as a file; imported once if found.
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// comindly's config folder.
pub fn config_dir() -> PathBuf {
    config_home().join("comindly")
}

/// A setting to write: a string, a flag or a number; `Remove` deletes the key.
pub enum Setting {
    Text(String),
    Flag(bool),
    Number(i64),
    Remove,
}

/// Apply `changes` to the stored preferences.
pub fn update(changes: &[(&str, Setting)]) -> Result<()> {
    let s = stored()?;
    let mut doc = serde_json::to_value(&s.doc)?;
    let map = doc.as_object_mut().context("preferences")?;
    for (key, value) in changes {
        anyhow::ensure!(
            KEYS.contains(key) && *key != "repos",
            "unknown preference `{key}` (one of {})",
            KEYS[..6].join(", ")
        );
        match value {
            Setting::Text(v) => drop(map.insert(key.to_string(), v.as_str().into())),
            Setting::Flag(v) => drop(map.insert(key.to_string(), (*v).into())),
            Setting::Number(v) => drop(map.insert(key.to_string(), (*v).into())),
            Setting::Remove => drop(map.remove(*key)),
        }
    }
    let next: Config = serde_json::from_value(doc).context("preferences")?;
    if next != s.doc {
        save(&next, s.version)?;
    }
    Ok(())
}

/// The web UI's access token for browsers: created once, so bookmarks keep working.
pub fn ui_token() -> Result<String> {
    let path = config_dir().join("ui-token");
    if let Ok(t) = std::fs::read_to_string(&path) {
        if !t.trim().is_empty() {
            return Ok(t.trim().to_string());
        }
    }
    std::fs::create_dir_all(path.parent().context("config dir")?)?;
    let token = crate::web::new_token();
    std::fs::write(&path, &token).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(token)
}

/// `$XDG_DATA_HOME/comindly`, else `~/.local/share/comindly`: the default index and managed clones.
/// comindly's cache folder on this machine: worktrees, verified signatures.
pub fn cache_dir() -> PathBuf {
    home_path("~/.cache/comindly")
}

pub fn data_dir() -> String {
    data_home().join("comindly").to_string_lossy().into_owned()
}

/// The default index location: the data folder itself.
pub fn default_index_dir() -> String {
    data_dir()
}

fn non_empty_env(key: &str) -> Option<String> {
    env(key).filter(|v| !v.is_empty())
}

/// An environment variable, if set.
pub fn env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home().join(".config"))
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home().join(".local").join("share"))
}

/// Expand a leading `~` to `$HOME`; leave URIs (`s3://…`) and absolute paths untouched.
pub fn expand_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        return home().join(rest).to_string_lossy().into_owned();
    }
    if s == "~" {
        return home().to_string_lossy().into_owned();
    }
    s.to_string()
}

/// The settings an index runs with here: its shared settings, then this machine's.
pub async fn index_settings(root: &str) -> crate::context::Settings {
    let team = crate::team::load(root)
        .await
        .map(|s| s.settings)
        .unwrap_or_default();
    crate::context::Settings::resolve(&team, crate::config::env)
}

/// An LLM client for `settings`, with the key for `team` from this machine or `server`.
pub fn llm_client(
    settings: &crate::context::Settings,
    team: Option<&str>,
    server: Option<&crate::server::Server>,
) -> Option<crate::llm::LlmClient> {
    let key = crate::credentials::llm_key(team, server).0;
    crate::llm::LlmClient::new(
        settings.llm_base_url.as_deref(),
        &settings.llm_model,
        key.as_deref(),
    )
    .ok()
}

fn home_path(p: &str) -> PathBuf {
    PathBuf::from(expand_tilde(p))
}

/// This machine's signing key: `COMINDLY_SIGNING_KEY` → `signing_key` → git `user.signingkey` when
/// `gpg.format=ssh`.
pub fn local_signing_key() -> Option<PathBuf> {
    if let Some(k) = env("COMINDLY_SIGNING_KEY").or_else(|| Config::load().signing_key) {
        return Some(home_path(&k));
    }
    let cfg = git2::Config::open_default().ok()?;
    if cfg.get_string("gpg.format").ok()? != "ssh" {
        return None;
    }
    let k = cfg.get_string("user.signingkey").ok()?;
    (!k.starts_with("key::")).then(|| home_path(&k))
}

/// `COMINDLY_ALLOWED_SIGNERS` → `allowed_signers` → git `gpg.ssh.allowedSignersFile`.
pub fn local_allowed_signers() -> Option<PathBuf> {
    let p = env("COMINDLY_ALLOWED_SIGNERS")
        .or_else(|| Config::load().allowed_signers)
        .or_else(|| {
            git2::Config::open_default()
                .ok()?
                .get_string("gpg.ssh.allowedSignersFile")
                .ok()
        })?;
    Some(home_path(&p)).filter(|p| p.is_file())
}

/// Signature checks with this machine's `allowed_signers`.
/// The deployment's token for `host`: `GH_TOKEN`/`GITHUB_TOKEN` for github.com, `GITLAB_TOKEN` for
/// gitlab.com and the hosts in `COMINDLY_GITLAB_HOSTS` (or their `_FILE`).
pub fn env_token(host: &str) -> Option<String> {
    let host = host.to_ascii_lowercase();
    let gitlab = host == "gitlab.com"
        || crate::server::env_secret("COMINDLY_GITLAB_HOSTS").is_some_and(|hosts| {
            hosts
                .split([',', ' '])
                .any(|h| h.trim().eq_ignore_ascii_case(&host))
        });
    let vars: &[&str] = if host == "github.com" {
        &["GH_TOKEN", "GITHUB_TOKEN"]
    } else if gitlab {
        &["GITLAB_TOKEN"]
    } else {
        &[]
    };
    vars.iter().find_map(|v| crate::server::env_secret(v))
}

/// Git tokens for indexing: the deployment's, then on a server the ones stored there.
pub fn git_credentials(
    server: Option<std::sync::Arc<crate::server::Server>>,
) -> std::sync::Arc<dyn crate::worktree::Credentials> {
    std::sync::Arc::new(move |host: &str| match &server {
        Some(s) => s.git_token(host),
        None => env_token(host),
    })
}

pub fn local_verifier() -> crate::signing::Verifier {
    crate::signing::Verifier::new(local_allowed_signers(), Some(cache_dir().join("verified")))
}

/// Who this machine records as: a pseudonymous actor id, and a name unless names are switched off
/// (`COMINDLY_AUTHOR_NAME` → `author_name` → git `user.name`).
pub fn local_identity() -> crate::ledger::Identity {
    let actor = env("COMINDLY_ACTOR").unwrap_or_else(|| {
        let who = std::env::var("USER").unwrap_or_default();
        let host = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_default();
        let h = blake3::hash(format!("{who}@{host}").as_bytes()).to_hex();
        format!("actor-{}", &h[..10])
    });
    let name = match env("COMINDLY_AUTHOR_NAME") {
        Some(n) => Some(n),
        None => {
            let cfg = Config::load();
            if cfg.record_names == Some(false) {
                None
            } else {
                cfg.author_name.or_else(|| {
                    git2::Config::open_default()
                        .ok()?
                        .get_string("user.name")
                        .ok()
                })
            }
        }
    };
    crate::ledger::Identity {
        actor,
        name: name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
    }
}

/// Settings older versions kept in `config.toml` that now belong to the index.
#[derive(Debug, Default, Deserialize)]
struct Legacy {
    llm_model: Option<String>,
    embed_model: Option<String>,
    llm_base_url: Option<String>,
    max_enrich: Option<usize>,
    max_flows: Option<usize>,
    format: Option<String>,
    embed: Option<bool>,
    enrich: Option<bool>,
    flows: Option<bool>,
    guide: Option<bool>,
    scip: Option<String>,
    auto_index: Option<bool>,
}

const LEGACY: [&str; 12] = [
    "llm_model",
    "embed_model",
    "llm_base_url",
    "max_enrich",
    "max_flows",
    "format",
    "embed",
    "enrich",
    "flows",
    "guide",
    "scip",
    "auto_index",
];

/// Fill what `team` leaves unset from settings an older `config.toml` held; `true` if any.
pub fn adopt_legacy(text: &str, team: &mut crate::team::TeamSettings) -> bool {
    let Ok(old) = toml::from_str::<Legacy>(text) else {
        return false;
    };
    let mut changed = false;
    macro_rules! take {
        ($($f:ident),*) => {$(
            if team.$f.is_none() && old.$f.is_some() {
                team.$f = old.$f.clone();
                changed = true;
            }
        )*};
    }
    take!(
        llm_model,
        embed_model,
        llm_base_url,
        max_enrich,
        max_flows,
        format,
        embed,
        enrich,
        flows,
        guide,
        scip,
        auto_index
    );
    changed
}

/// Keys older files may carry that nothing reads any more; imports skip them.
const OBSOLETE: [&str; 2] = ["aws_profile", "sso_auto_login"];

/// What an import changed.
#[derive(Debug, Default)]
pub struct Imported {
    pub changed: bool,
    /// Preferences already set to something else, left as they were.
    pub kept: Vec<String>,
    /// Index settings the file carried, moved into the index's shared settings.
    pub moved: bool,
}

/// Import a preferences file. Without `overwrite` it fills only what is unset; with it, the file
/// replaces the preferences. Index settings older files carried move to the index once.
pub fn import(text: &str, overwrite: bool) -> Result<Imported> {
    let table: toml::Table = toml::from_str(text).context("not a TOML file")?;
    let unknown: Vec<&str> = table
        .keys()
        .map(String::as_str)
        .filter(|k| !KEYS.contains(k) && !LEGACY.contains(k) && !OBSOLETE.contains(k))
        .collect();
    anyhow::ensure!(
        unknown.is_empty(),
        "unknown setting(s): {} (preferences are {})",
        unknown.join(", "),
        KEYS.join(", ")
    );
    let file: Config = toml::from_str(text)?;
    let s = stored()?;
    let mut out = Imported::default();
    let next = if overwrite {
        file
    } else {
        let mut next = s.doc.clone();
        macro_rules! fill {
            ($($f:ident),*) => {$(
                match (&next.$f, &file.$f) {
                    (None, Some(_)) => next.$f = file.$f.clone(),
                    (Some(a), Some(b)) if a != b => out.kept.push(stringify!($f).to_string()),
                    _ => {}
                }
            )*};
        }
        fill!(
            index_dir,
            signing_key,
            allowed_signers,
            cache_ttl_secs,
            author_name,
            record_names
        );
        if next.repos.is_empty() {
            next.repos = file.repos.clone();
        } else if !file.repos.is_empty() && file.repos != next.repos {
            out.kept.push("repos".into());
        }
        next
    };
    if next != s.doc {
        save(&next, s.version)?;
        out.changed = true;
    }
    let root = next.index_dir(None);
    if LEGACY.iter().any(|k| table.contains_key(*k)) && !crate::cache::is_remote(&root) {
        let me = local_identity();
        let by = me.name.unwrap_or(me.actor);
        out.moved = crate::rt::block_on(async {
            let stored = crate::team::load(&root).await?;
            let mut settings = stored.settings;
            let moved = adopt_legacy(text, &mut settings);
            if moved {
                crate::team::save_as(&root, &settings, stored.version, &by).await?;
            }
            anyhow::Ok(moved)
        })?;
    }
    Ok(out)
}

/// The preferences as a TOML file that [`import`] reads back.
pub fn export(cfg: &Config) -> Result<String> {
    Ok(format!(
        "# comindly preferences: `comindly config import <file> --override` applies this file.\n\n{}",
        toml::to_string_pretty(cfg)?
    ))
}

/// An older `config.toml` is imported once and renamed; one that appears later is pointed out.
pub fn adopt_file() {
    let path = config_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    match stored() {
        Ok(s) if s.version.is_some() => {
            crate::ui::warn(&format!(
                "{} is not read; apply it with `comindly config import {}`",
                path.display(),
                path.display()
            ));
            return;
        }
        Err(_) => return,
        Ok(_) => {}
    }
    let done = import(&text, true).and_then(|_| {
        Ok(std::fs::rename(
            &path,
            path.with_extension("toml.imported"),
        )?)
    });
    match done {
        Ok(()) => crate::ui::note(&format!(
            "imported {} into your preferences (`comindly config`); the file is kept as {}",
            path.display(),
            path.with_extension("toml.imported").display()
        )),
        Err(e) => crate::ui::warn(&format!("could not import {}: {e:#}", path.display())),
    }
}

/// Save an index's shared settings as you, if nobody saved since `expected`.
pub async fn save_team(
    root: &str,
    settings: &crate::team::TeamSettings,
    expected: Option<u64>,
) -> Result<u64> {
    let me = local_identity();
    crate::team::save_as(
        root,
        settings,
        expected,
        me.name.as_deref().unwrap_or(&me.actor),
    )
    .await
}
