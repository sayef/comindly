//! What the command line reads from git on this machine, through the `git` command.

use std::path::Path;

use anyhow::{bail, Context, Result};

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("run git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn head_commit(dir: &Path) -> Result<String> {
    Ok(git(dir, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// A changed file and its changed line ranges (none: the whole file).
pub type FileHunks = (String, Vec<(u32, u32)>);

/// Uncommitted changes (staged, unstaged and untracked) against `base`, with changed line ranges.
pub fn working_changes(dir: &Path, base: &str) -> Result<Vec<FileHunks>> {
    let mut out: Vec<FileHunks> = Vec::new();
    let diff = git(dir, &["diff", "--no-color", "--no-ext-diff", "-U0", base])?;
    for line in diff.lines() {
        if let Some(p) = line.strip_prefix("+++ b/") {
            out.push((p.to_string(), Vec::new()));
        } else if let Some(h) = line.strip_prefix("@@ ") {
            // `@@ -a,b +c,d @@`: the new side starts at c and spans d lines (1 when omitted).
            let new = h.split_whitespace().find(|p| p.starts_with('+'));
            if let (Some(new), Some(last)) = (new, out.last_mut()) {
                let (start, len) = new[1..].split_once(',').unwrap_or((&new[1..], "1"));
                let (start, len): (u32, u32) = (start.parse()?, len.parse()?);
                let start = start.max(1);
                last.1.push((start, start + len.max(1) - 1));
            }
        }
    }
    for p in git(dir, &["ls-files", "--others", "--exclude-standard"])?.lines() {
        out.push((p.to_string(), Vec::new()));
    }
    Ok(out)
}

/// Files added, modified and deleted since `base`.
pub fn changed_files(dir: &Path, base: &str) -> Result<[Vec<String>; 3]> {
    let mut v: [Vec<String>; 3] = Default::default();
    for line in git(
        dir,
        &["diff", "--name-status", "--no-renames", base, "HEAD"],
    )?
    .lines()
    {
        let Some((status, path)) = line.split_once('\t') else {
            continue;
        };
        let i = match status.chars().next() {
            Some('A') => 0,
            Some('D') => 2,
            _ => 1,
        };
        v[i].push(path.to_string());
    }
    Ok(v)
}
