//! comindly-git — git change detection for incremental indexing.
//!
//! Uses `git2` (vendored libgit2, statically linked → single binary preserved). At index time
//! the working tree and `.git` are present; in CI indexing runs on a pushed commit, so
//! incremental indexing diffs the last-indexed commit against the current HEAD and re-processes
//! only what changed (added / modified / deleted files).

use std::path::Path;

use anyhow::{Context, Result};
use git2::{Delta, Repository};

/// Files that changed between two commits, split by how the index should treat them.
#[derive(Debug, Default, Clone)]
pub struct ChangeSet {
    /// New files → parse and add.
    pub added: Vec<String>,
    /// Changed files → re-parse and replace their symbols/edges.
    pub modified: Vec<String>,
    /// Removed files → drop their symbols/edges.
    pub deleted: Vec<String>,
}

impl ChangeSet {
    pub fn total(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }

    /// Files whose current content must be (re)parsed: added + modified.
    pub fn to_parse(&self) -> impl Iterator<Item = &String> {
        self.added.iter().chain(self.modified.iter())
    }

    /// Files whose old symbols must be dropped: modified + deleted.
    pub fn to_drop(&self) -> impl Iterator<Item = &String> {
        self.modified.iter().chain(self.deleted.iter())
    }
}

fn path_string(p: Option<&Path>) -> Option<String> {
    p.map(|p| p.to_string_lossy().replace('\\', "/"))
}

/// The current HEAD commit SHA of the repo at `repo_path` — stored as the index version so the
/// next run knows where to diff from.
pub fn head_commit(repo_path: &Path) -> Result<String> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("open git repo at {}", repo_path.display()))?;
    let commit = repo.head()?.peel_to_commit()?;
    Ok(commit.id().to_string())
}

/// Files changed between `base_sha` and current HEAD. Renames are recorded as a delete of the
/// old path plus an add of the new path (so downstream drop/add logic stays simple).
pub fn changed_files(repo_path: &Path, base_sha: &str) -> Result<ChangeSet> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("open git repo at {}", repo_path.display()))?;
    let base_tree = repo
        .revparse_single(base_sha)
        .with_context(|| format!("resolve base commit {base_sha}"))?
        .peel_to_commit()?
        .tree()?;
    let head_tree = repo.head()?.peel_to_commit()?.tree()?;

    let mut opts = git2::DiffOptions::new();
    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut opts))?;

    let mut cs = ChangeSet::default();
    for d in diff.deltas() {
        let new_p = path_string(d.new_file().path());
        let old_p = path_string(d.old_file().path());
        match d.status() {
            Delta::Added | Delta::Copied => cs.added.extend(new_p),
            Delta::Deleted => cs.deleted.extend(old_p),
            Delta::Modified | Delta::Typechange => cs.modified.extend(new_p),
            Delta::Renamed => {
                cs.deleted.extend(old_p);
                cs.added.extend(new_p);
            }
            _ => {}
        }
    }
    Ok(cs)
}

/// One commit's file changes (first-parent history).
#[derive(Debug, Clone)]
pub struct CommitFiles {
    pub sha: String,
    pub subject: String,
    pub added: Vec<String>,
    pub changed: Vec<String>,
}

/// The last `limit` first-parent commits with their added and changed files.
pub fn history(repo_path: &Path, limit: usize) -> Result<Vec<CommitFiles>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("open git repo at {}", repo_path.display()))?;
    let mut walk = repo.revwalk()?;
    walk.push_head()?;
    walk.simplify_first_parent()?;
    let mut out = Vec::new();
    for oid in walk.take(limit) {
        let commit = repo.find_commit(oid?)?;
        let tree = commit.tree()?;
        let parent = commit.parents().next().map(|p| p.tree()).transpose()?;
        let diff = repo.diff_tree_to_tree(parent.as_ref(), Some(&tree), None)?;
        let mut c = CommitFiles {
            sha: commit.id().to_string(),
            subject: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            added: Vec::new(),
            changed: Vec::new(),
        };
        for d in diff.deltas() {
            let Some(p) = path_string(d.new_file().path()) else {
                continue;
            };
            if matches!(d.status(), Delta::Added | Delta::Copied | Delta::Renamed) {
                c.added.push(p.clone());
            }
            if !matches!(d.status(), Delta::Deleted) {
                c.changed.push(p);
            }
        }
        out.push(c);
    }
    Ok(out)
}

/// A changed file and its changed new-side line ranges.
pub type FileHunks = (String, Vec<(u32, u32)>);

/// Uncommitted changes (staged and unstaged) against `base`.
pub fn working_changes(repo_path: &Path, base: &str) -> Result<Vec<FileHunks>> {
    let repo = Repository::open(repo_path)
        .with_context(|| format!("open git repo at {}", repo_path.display()))?;
    let tree = repo.revparse_single(base)?.peel_to_tree()?;
    let mut opts = git2::DiffOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let diff = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))?;
    let out: std::cell::RefCell<Vec<FileHunks>> = Default::default();
    diff.foreach(
        &mut |d, _| {
            if let Some(p) = path_string(d.new_file().path()) {
                out.borrow_mut().push((p, Vec::new()));
            }
            true
        },
        None,
        Some(&mut |d, h| {
            let mut v = out.borrow_mut();
            if let (Some(p), Some(last)) = (path_string(d.new_file().path()), v.last_mut()) {
                if last.0 == p {
                    let start = h.new_start().max(1);
                    last.1.push((start, start + h.new_lines().max(1) - 1));
                }
            }
            true
        }),
        None,
    )?;
    Ok(out.into_inner())
}
