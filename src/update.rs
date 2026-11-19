//! `comindly update`: replace this command line, and the local engine when it is installed, with
//! the newest release (betas too while this one is a beta), checked against the release's checksum.

use std::process::ExitCode;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `1.2.3` or `1.2.3-beta.4` as comparable parts; a release sorts after its betas.
fn parts(v: &str) -> Option<(u64, u64, u64, bool, u64)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((c, p)) => (c, Some(p)),
        None => (v, None),
    };
    let mut n = core.split('.').map(|x| x.parse::<u64>().ok());
    let (a, b, c) = (n.next()??, n.next()??, n.next()??);
    let beta = pre
        .map(|p| {
            p.rsplit('.')
                .next()
                .and_then(|x| x.parse().ok())
                .unwrap_or(0)
        })
        .unwrap_or(0);
    Some((a, b, c, pre.is_none(), beta))
}

/// Whether `a` is a later version than `b`.
fn newer(a: &str, b: &str) -> bool {
    matches!((parts(a), parts(b)), (Some(x), Some(y)) if x > y)
}

/// The newest release this command line should move to; betas count while it is a beta itself.
fn latest(releases: &[Release], current: &str) -> Option<String> {
    let betas = current.contains('-');
    releases
        .iter()
        .filter(|r| !r.draft && (betas || !r.prerelease))
        .map(|r| r.tag_name.trim_start_matches('v').to_string())
        .filter(|v| parts(v).is_some())
        .max_by(|x, y| parts(x).cmp(&parts(y)))
}

async fn releases() -> Result<Vec<Release>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=30",
        crate::engine::REPO
    );
    let r = reqwest::Client::new()
        .get(&url)
        .header("user-agent", "comindly-cli")
        .header("accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()
        .context("ask GitHub for comindly's releases")?;
    Ok(r.json().await?)
}

pub fn update(version: Option<String>, check: bool) -> ExitCode {
    match crate::commands::block(run(version, check)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            crate::term::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

async fn run(version: Option<String>, check: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let asked = version.is_some();
    let want = match version {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => latest(&releases().await?, current).context("no release found")?,
    };
    // A version asked for by name may be older: going back is allowed then.
    if want == current || (!asked && !newer(&want, current)) {
        crate::term::ok(&format!("comindly {current} is up to date"));
        return Ok(());
    }
    if check {
        crate::term::note(&format!(
            "comindly {want} is available (this is {current}); run `comindly update`"
        ));
        return Ok(());
    }
    let me = std::env::current_exe()?;
    crate::engine::fetch("comindly", &want, &me).await?;
    crate::term::ok(&format!("comindly {current} → {want} ({})", me.display()));
    // The engine is updated where it lies, beside this command line, when it is installed.
    let engine = me
        .parent()
        .map(|d| {
            d.join(if cfg!(windows) {
                "comindly-engine.exe"
            } else {
                "comindly-engine"
            })
        })
        .filter(|p| p.is_file());
    if let Some(engine) = engine {
        let was_running = crate::engine::running().is_some();
        if was_running {
            crate::engine::stop()?;
        }
        crate::engine::fetch("comindly-engine", &want, &engine).await?;
        crate::term::ok(&format!("comindly-engine → {want}"));
        if was_running {
            crate::term::note(
                "the local engine was stopped; it starts again with the next command",
            );
        }
    }
    crate::term::note(
        "plugins installed with `init-agents --install` keep running; run it again with --yes to refresh their guidance",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rel(tag: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease,
        }
    }

    #[test]
    fn versions_order_with_betas_before_their_release() {
        assert!(newer("0.5.0-beta.23", "0.5.0-beta.22"));
        assert!(newer("0.5.0-beta.10", "0.5.0-beta.9"));
        assert!(newer("0.5.0", "0.5.0-beta.30"));
        assert!(newer("v0.6.0", "0.5.9"));
        assert!(!newer("0.5.0-beta.2", "0.5.0-beta.2"));
        assert!(!newer("nonsense", "0.5.0"));
    }

    #[test]
    fn a_beta_moves_to_betas_and_a_release_only_to_releases() {
        let all = [
            rel("v0.5.0-beta.23", true),
            rel("v0.5.0-beta.9", true),
            rel("v0.4.2", false),
        ];
        assert_eq!(
            latest(&all, "0.5.0-beta.22").as_deref(),
            Some("0.5.0-beta.23")
        );
        assert_eq!(latest(&all, "0.4.1").as_deref(), Some("0.4.2"));
    }
}
