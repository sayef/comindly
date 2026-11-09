//! Which engine the command line talks to: `local` (the engine on this machine) or a named server,
//! each with its own sign-in. `comindly use` switches.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use comindly_api::client::Client;
use serde::{Deserialize, Serialize};

pub const LOCAL: &str = "local";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    /// The server's address; empty for `local`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Profiles {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

/// comindly's config folder, as the engine keeps it.
pub fn config_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| home.join(".config"))
        .join("comindly")
}

fn path() -> PathBuf {
    config_dir().join("profiles.toml")
}

impl Profiles {
    pub fn load() -> Profiles {
        match std::fs::read_to_string(path()) {
            Ok(t) => toml::from_str(&t).unwrap_or_default(),
            Err(_) => Profiles::migrated(),
        }
    }

    /// A sign-in from before profiles becomes the `default` profile.
    fn migrated() -> Profiles {
        #[derive(Deserialize)]
        struct Old {
            server: Option<Profile>,
        }
        let mut p = Profiles::default();
        let old = std::fs::read_to_string(config_dir().join("credentials"))
            .ok()
            .and_then(|t| toml::from_str::<Old>(&t).ok())
            .and_then(|o| o.server);
        if let Some(server) = old {
            p.profiles.insert("default".into(), server);
            p.active = Some("default".into());
            let _ = p.save();
        }
        p
    }

    pub fn save(&self) -> Result<()> {
        let p = path();
        std::fs::create_dir_all(p.parent().context("config dir")?)?;
        let text = format!(
            "# comindly's engines and their sign-ins (`comindly use`). Keep this file private.\n\n{}",
            toml::to_string(self)?
        );
        let tmp = p.with_extension("tmp");
        let mut open = std::fs::OpenOptions::new();
        open.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        std::io::Write::write_all(&mut open.open(&tmp)?, text.as_bytes())?;
        std::fs::rename(&tmp, &p).with_context(|| format!("write {}", p.display()))
    }
}

/// The profile a command uses: `--use`, else `COMINDLY_PROFILE`, else the active one, else local.
pub fn chosen(flag: Option<&str>) -> String {
    flag.map(str::to_string)
        .or_else(|| {
            std::env::var("COMINDLY_PROFILE")
                .ok()
                .filter(|p| !p.is_empty())
        })
        .or_else(|| Profiles::load().active)
        .unwrap_or_else(|| LOCAL.into())
}

/// A client for the chosen engine. `COMINDLY_SERVER` with `COMINDLY_TOKEN` stands in for a
/// profile, e.g. in CI. The local engine is started when it is not running.
pub fn client(flag: Option<&str>) -> Result<Client> {
    if flag.is_none() {
        if let Some(url) = std::env::var("COMINDLY_SERVER")
            .ok()
            .filter(|u| !u.is_empty())
        {
            let token = std::env::var("COMINDLY_TOKEN")
                .ok()
                .filter(|t| !t.is_empty());
            return Ok(Client::new(&url, token));
        }
    }
    let name = chosen(flag);
    if name == LOCAL {
        let url = crate::engine::ensure_running()?;
        return Ok(Client::new(&url, crate::engine::local_token()));
    }
    let profiles = Profiles::load();
    let Some(p) = profiles.profiles.get(&name) else {
        bail!("no profile named `{name}`: add it with `comindly use <url> {name}`");
    };
    let token = std::env::var("COMINDLY_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .or_else(|| p.token.clone());
    Ok(Client::new(&p.url, token))
}

/// Remember the sign-in for profile `name`; `None` forgets it.
pub fn set_sign_in(name: &str, token: Option<String>, email: Option<String>) -> Result<()> {
    let mut all = Profiles::load();
    let p = all.profiles.get_mut(name).context("no such profile")?;
    p.token = token;
    p.email = email;
    all.save()
}
