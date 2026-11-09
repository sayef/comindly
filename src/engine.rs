//! The engine on this machine: found next to `comindly` or on PATH, started in the background when
//! a command needs it, and installed from the release of the same version.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

const REPO: &str = "sayef/comindly";

/// What a running local engine wrote down about itself.
#[derive(Debug, Deserialize)]
pub struct Running {
    pub url: String,
    pub pid: u32,
    pub version: String,
}

fn running_file() -> PathBuf {
    crate::profile::config_dir().join("engine.json")
}

/// The local engine, if one is running and answers.
pub fn running() -> Option<Running> {
    let r: Running = serde_json::from_str(&std::fs::read_to_string(running_file()).ok()?).ok()?;
    let addr = r.url.trim_start_matches("http://").to_string();
    let addr = std::net::ToSocketAddrs::to_socket_addrs(&addr)
        .ok()?
        .next()?;
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    Some(r)
}

/// The token the local engine accepts; it creates it on first start.
pub fn local_token() -> Option<String> {
    std::fs::read_to_string(crate::profile::config_dir().join("ui-token"))
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "comindly-engine.exe"
    } else {
        "comindly-engine"
    }
}

/// The engine binary: beside this one, else on PATH.
pub fn binary() -> Option<PathBuf> {
    let beside = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(exe_name())));
    beside.filter(|p| p.is_file()).or_else(|| {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|d| d.join(exe_name()))
                .find(|p| p.is_file())
        })
    })
}

/// The local engine's address, starting it when it is not running.
pub fn ensure_running() -> Result<String> {
    if let Some(r) = running() {
        return Ok(r.url);
    }
    start()?;
    Ok(running().context("the local engine did not start")?.url)
}

/// Start the local engine in the background and wait until it answers; installs it first when
/// it is missing.
pub fn start() -> Result<()> {
    let bin = match binary() {
        Some(b) => b,
        None => {
            crate::term::note("the local engine is not installed yet; installing it");
            crate::commands::block(install())?
        }
    };
    let log_dir = std::env::temp_dir();
    let log = std::fs::File::create(log_dir.join("comindly-engine.log"))?;
    let mut cmd = std::process::Command::new(&bin);
    cmd.stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .env_remove("COMINDLY_PUBLIC_URL");
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .spawn()
        .with_context(|| format!("start {}", bin.display()))?;
    crate::term::note("starting the local engine…");
    for _ in 0..150 {
        if running().is_some() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            bail!(
                "the local engine stopped ({status}); see {}",
                log_dir.join("comindly-engine.log").display()
            );
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    bail!("the local engine did not answer within 30 seconds")
}

/// Stop the local engine; false when none was running.
pub fn stop() -> Result<bool> {
    let Some(r) = running() else {
        return Ok(false);
    };
    let ok = std::process::Command::new("kill")
        .arg(r.pid.to_string())
        .status()
        .is_ok_and(|s| s.success());
    anyhow::ensure!(ok, "could not stop the engine (pid {})", r.pid);
    for _ in 0..50 {
        if running().is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(true)
}

fn target() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        a => bail!("no engine build for {a}"),
    };
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "linux" => "unknown-linux-gnu",
        o => bail!("no engine build for {o}"),
    };
    Ok(format!("{arch}-{os}"))
}

/// Download the engine of this version from the GitHub release, check it, and put it beside
/// `comindly`.
pub async fn install() -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let asset = format!("comindly-engine-{}.tar.gz", target()?);
    let url = format!("https://github.com/{REPO}/releases/download/v{version}/{asset}");
    crate::term::note(&format!("downloading {url}"));
    let http = reqwest::Client::new();
    let get = |u: String| {
        let http = http.clone();
        async move {
            let r = http.get(&u).send().await?.error_for_status()?;
            anyhow::Ok(r.bytes().await?)
        }
    };
    let archive = get(url.clone())
        .await
        .with_context(|| format!("download {url}"))?;
    let sum = get(format!("{url}.sha256")).await?;
    let want = String::from_utf8_lossy(&sum)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let dir = std::env::temp_dir().join(format!("comindly-engine-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(&asset);
    std::fs::write(&file, &archive)?;
    let got = sha256(&file)?;
    anyhow::ensure!(got == want, "checksum mismatch for {asset}");
    let ok = std::process::Command::new("tar")
        .args(["-xzf"])
        .arg(&file)
        .arg("-C")
        .arg(&dir)
        .status()?
        .success();
    anyhow::ensure!(ok, "could not unpack {asset}");
    let into = std::env::current_exe()?
        .parent()
        .context("no folder for comindly")?
        .join(exe_name());
    // A new file renamed into place: macOS kills a binary overwritten where it lies.
    let tmp = into.with_extension("new");
    std::fs::copy(dir.join(exe_name()), &tmp)
        .with_context(|| format!("write {}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&tmp, &into).with_context(|| format!("write {}", into.display()))?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(into)
}

fn sha256(file: &std::path::Path) -> Result<String> {
    let out = std::process::Command::new("shasum")
        .args(["-a", "256"])
        .arg(file)
        .output()
        .or_else(|_| std::process::Command::new("sha256sum").arg(file).output())
        .context("no shasum or sha256sum to check the download")?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string())
}
