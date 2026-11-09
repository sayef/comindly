//! Starting an engine: a team server when `COMINDLY_PUBLIC_URL` is set, else a local one for this
//! machine's command line and app.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Where a running local engine listens, for the command line to find it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Running {
    pub url: String,
    pub pid: u32,
    pub version: String,
}

/// `<config dir>/engine.json`, written while a local engine runs.
pub fn running_file() -> PathBuf {
    crate::config::config_dir().join("engine.json")
}

/// The local engine recorded as running, if its process is still alive.
pub fn running() -> Option<Running> {
    let text = std::fs::read_to_string(running_file()).ok()?;
    let r: Running = serde_json::from_str(&text).ok()?;
    alive(r.pid).then_some(r)
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Serve the index at `from` until the process ends; `port` 0 takes a free one.
pub fn serve(from: Option<&str>, host: &str, port: u16) -> Result<()> {
    let root = crate::config::Config::load().index_dir(from);
    let data = crate::view::Data::load_or_empty(&root, &crate::config::local_verifier());
    let addr = format!("{host}:{port}");
    let listener = std::net::TcpListener::bind(&addr).with_context(|| format!("bind {addr}"))?;
    let addr = listener.local_addr()?.to_string();
    if let Some(cfg) = crate::server::ServerConfig::from_env()? {
        let (url, sign_in) = (cfg.public_url.clone(), cfg.sign_in.clone());
        let server = crate::server::Server::open(&root, cfg).context("open the server table")?;
        migrate(&server, &root)?;
        // What an admin switched in the app outlasts the deployment's `COMINDLY_LOG_SKIP`.
        if let Some(skip) = server.settings.log_skip() {
            let off = crate::telemetry::parse_skip(&skip);
            for kind in ["access", "tool"] {
                crate::telemetry::set_logs(kind, !off.iter().any(|k| k == kind));
            }
        }
        crate::ui::ok(&format!("comindly server for {url}, listening on {addr}"));
        let ways: Vec<&str> = sign_in
            .iter()
            .map(|m| match m {
                crate::server::SignIn::Email => "emailed links",
                crate::server::SignIn::Code => "codes admins hand out",
                crate::server::SignIn::Oidc => "the OpenID Connect provider",
            })
            .collect();
        crate::ui::note(&format!("members sign in with {}", ways.join(", ")));
        return crate::web::serve(data, listener, crate::web::Access::Server(Arc::new(server)));
    }
    let token = crate::config::ui_token()?;
    let bound = listener.local_addr()?.port();
    let running = Running {
        url: format!("http://{host}:{bound}"),
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    std::fs::write(running_file(), serde_json::to_string(&running)?)?;
    crate::ui::ok(&format!(
        "comindly engine on {} (Ctrl-C to stop)",
        running.url
    ));
    let served = crate::web::serve(data, listener, crate::web::Access::Local(token));
    let _ = std::fs::remove_file(running_file());
    served
}

/// Moves a server's kept `config.toml` into its settings and the index's, once.
fn migrate(server: &crate::server::Server, root: &str) -> Result<()> {
    let Some(text) = server.settings.legacy_config() else {
        return Ok(());
    };
    if server.settings.repos().is_empty() {
        let old: crate::config::Config = toml::from_str(&text).unwrap_or_default();
        server.settings.set_repos("upgrade", &old.repos)?;
    }
    crate::rt::block_on(async {
        let stored = crate::team::load(root).await?;
        let mut settings = stored.settings;
        if crate::config::adopt_legacy(&text, &mut settings) {
            crate::team::save_as(root, &settings, stored.version, "upgrade").await?;
        }
        anyhow::Ok(())
    })?;
    server.settings.drop_legacy_config()?;
    crate::ui::note("moved the server's kept config.toml into its settings and the index's");
    Ok(())
}
