//! Local mirror of a remote (S3) index for latency-sensitive readers such as agent hooks: the
//! graph as a local Lance dataset and the ledger as one JSON file, refreshed in the background
//! when stale. Writes still go to the remote store.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ledger::Record;
use crate::model::{Edge, Symbol};

/// Default seconds between freshness checks against the remote index.
pub const DEFAULT_TTL: u64 = 300;

#[derive(Debug, Default, Serialize, Deserialize)]
struct Meta {
    /// `(symbols, edges)` Lance versions the local graph was copied from.
    versions: Option<(u64, u64)>,
}

pub use crate::storage::is_remote;

/// Cache directory for a remote index root.
pub fn dir(index_root: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let key = blake3::hash(index_root.as_bytes()).to_hex();
    Some(
        PathBuf::from(home)
            .join(".cache/comindly/remote")
            .join(&key[..16]),
    )
}

/// The cached graph and ledger, when a mirror exists.
pub fn read(cache: &Path) -> Option<(Vec<Symbol>, Vec<Edge>, Vec<Record>)> {
    let graph = cache.join("_graph");
    if !graph.is_dir() {
        return None;
    }
    let (symbols, edges) = crate::index::read_graph_blocking(graph.to_str()?).ok()?;
    let records = std::fs::read(cache.join("ledger.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    Some((symbols, edges, records))
}

/// Whether the mirror is due for a freshness check (missing, or older than `ttl`).
pub fn stale(cache: &Path, ttl: Duration) -> bool {
    std::fs::metadata(cache.join("checked"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|age| age > ttl)
}

/// Mark the mirror stale so the next reader refreshes it (after a local write).
pub fn invalidate(index_root: &str) {
    if let Some(d) = dir(index_root) {
        let _ = std::fs::remove_file(d.join("checked"));
    }
}

/// Sync the mirror: copy the graph when its remote version changed, re-read the ledger.
/// Returns whether the graph was copied.
pub fn refresh(index_root: &str, cache: &Path) -> Result<bool> {
    std::fs::create_dir_all(cache)?;
    let lock = cache.join("refresh.lock");
    // One refresher at a time; a lock older than 10 minutes is from a crashed run.
    let held = std::fs::metadata(&lock)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(600));
    if held {
        return Ok(false);
    }
    std::fs::write(&lock, b"")?;
    let out = sync(index_root, cache);
    let _ = std::fs::remove_file(&lock);
    out
}

fn sync(index_root: &str, cache: &Path) -> Result<bool> {
    let remote_graph = format!("{}/_graph", index_root.trim_end_matches('/'));
    let meta_path = cache.join("meta.json");
    let mut meta: Meta = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let now = crate::index::latest_versions_blocking(&remote_graph).context("remote index")?;
    let copied = meta.versions != Some(now) || !cache.join("_graph").is_dir();
    if copied {
        let (symbols, edges) = crate::index::read_graph_blocking(&remote_graph)?;
        let tmp = cache.join("_graph.new");
        let _ = std::fs::remove_dir_all(&tmp);
        crate::index::write_graph_blocking(tmp.to_str().context("cache path")?, &symbols, &edges)?;
        let _ = std::fs::remove_dir_all(cache.join("_graph"));
        std::fs::rename(&tmp, cache.join("_graph"))?;
        meta.versions = Some(now);
    }
    let records = crate::ledger::all_blocking(index_root)?;
    let tmp = cache.join("ledger.json.new");
    std::fs::write(&tmp, serde_json::to_vec(&records)?)?;
    std::fs::rename(&tmp, cache.join("ledger.json"))?;
    std::fs::write(&meta_path, serde_json::to_vec(&meta)?)?;
    std::fs::write(cache.join("checked"), b"")?;
    Ok(copied)
}

/// Start `comindly cache refresh` detached, so the caller never waits on the remote store.
pub fn refresh_in_background(index_root: &str) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = std::process::Command::new(exe)
        .args(["cache", "refresh", "--index-dir", index_root])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirror_follows_the_remote() {
        let base = std::env::temp_dir().join(format!("comindly-cache-{}", std::process::id()));
        let remote = base.join("remote");
        let cache = base.join("cache");
        let root = remote.to_str().unwrap();
        let sym = |name: &str| crate::model::Symbol {
            id: crate::model::GlobalSymbolId {
                scheme: "t".into(),
                package_manager: ".".into(),
                package: "r".into(),
                version: ".".into(),
                descriptor: format!("m/{name}()."),
            },
            name: name.into(),
            kind: crate::model::SymbolKind::Function,
            language: crate::model::Language::Python,
            repo: crate::model::RepoId("r".into()),
            file_path: "m.py".into(),
            range: crate::model::Range {
                start: crate::model::Position { line: 1, column: 0 },
                end: crate::model::Position { line: 2, column: 0 },
            },
            signature: None,
            docstring: None,
            fingerprint: None,
        };
        let graph = format!("{root}/_graph");
        crate::index::write_graph_blocking(&graph, &[sym("a")], &[]).unwrap();
        assert!(stale(&cache, Duration::from_secs(300)));
        assert!(refresh(root, &cache).unwrap(), "first sync copies");
        assert!(!stale(&cache, Duration::from_secs(300)));
        assert!(
            !refresh(root, &cache).unwrap(),
            "unchanged remote is not copied again"
        );
        crate::index::write_graph_blocking(&graph, &[sym("a"), sym("b")], &[]).unwrap();
        assert!(
            refresh(root, &cache).unwrap(),
            "a new remote version is copied"
        );
        let (symbols, _, records) = read(&cache).unwrap();
        assert_eq!(symbols.len(), 2);
        assert!(records.is_empty());
        std::fs::remove_dir_all(&base).ok();
    }
}
