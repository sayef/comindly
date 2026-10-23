//! comindly's own checkout of each repository's default branch: a detached git worktree under
//! `~/.cache/comindly/worktrees`, so indexing never touches your working folder or branches. Uses the
//! `git` command, so your SSH keys and credential helpers apply.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// A checkout ready to index.
pub struct Checkout {
    pub dir: PathBuf,
    /// What was checked out, e.g. `origin/main`.
    pub target: String,
    pub commit: String,
    /// Set when fetching failed and the last fetched state was used instead.
    pub stale: Option<String>,
}

/// An HTTPS username and access token, handed to git for one command only.
#[derive(Clone)]
pub struct Auth {
    pub username: String,
    pub token: String,
}

/// Returned when an HTTPS remote needs a token that neither git's credential helper nor the environment has.
#[derive(Debug)]
pub struct NeedsToken {
    pub host: String,
    pub username: String,
}

impl std::fmt::Display for NeedsToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} needs an access token", self.host)
    }
}

impl std::error::Error for NeedsToken {}

fn https_host(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let authority = rest.split('/').next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// The username a host expects with an access token.
pub fn token_user(host: &str) -> &'static str {
    if host == "github.com" {
        "x-access-token"
    } else {
        "oauth2"
    }
}

/// Access tokens for HTTPS remotes by host; without one, git's own credential helper answers.
pub trait Credentials: Send + Sync {
    fn token(&self, host: &str) -> Option<String>;
}

/// No tokens: git's credential helper and SSH keys only.
pub struct NoCredentials;

impl Credentials for NoCredentials {
    fn token(&self, _: &str) -> Option<String> {
        None
    }
}

impl<F: Fn(&str) -> Option<String> + Send + Sync> Credentials for F {
    fn token(&self, host: &str) -> Option<String> {
        self(host)
    }
}

/// A token for cloning or fetching `url` without asking.
pub fn auth_for(url: &str, creds: &dyn Credentials) -> Option<Auth> {
    let host = https_host(url)?;
    creds.token(&host).map(|token| Auth {
        username: token_user(&host).into(),
        token,
    })
}

fn auth_failed(stderr: &str) -> bool {
    [
        "could not read Username",
        "could not read Password",
        "Authentication failed",
        "Access denied",
        "HTTP Basic",
        "returned error: 401",
        "returned error: 403",
    ]
    .iter()
    .any(|m| stderr.contains(m))
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    git_auth(dir, args, None)
}

/// Run git; with `auth`, a credential helper that answers with it is tried after your own helpers.
fn git_auth(dir: &Path, args: &[&str], auth: Option<&Auth>) -> Result<String> {
    finish(args, command(dir, args, auth).output())
}

/// A git command in `dir` that never prompts and gives up on a stalled transfer.
fn command(dir: &Path, args: &[&str], auth: Option<&Auth>) -> Command {
    let mut cmd = Command::new("git");
    if let Some(a) = auth {
        cmd.arg("-c")
            .arg(r#"credential.helper=!f() { test "$1" = get && printf 'username=%s\npassword=%s\n' "$COMINDLY_GIT_USER" "$COMINDLY_GIT_TOKEN"; }; f"#)
            .env("COMINDLY_GIT_USER", &a.username)
            .env("COMINDLY_GIT_TOKEN", &a.token);
    }
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        // Give up on a stalled fetch instead of waiting on a struggling server.
        .env("GIT_HTTP_LOW_SPEED_LIMIT", "1000")
        .env("GIT_HTTP_LOW_SPEED_TIME", "20");
    cmd
}

fn finish(args: &[&str], out: std::io::Result<std::process::Output>) -> Result<String> {
    let out = out.context("run git (is it installed?)")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The repository's name: its folder's name.
pub fn name(repo: &Path) -> String {
    repo.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".into())
}

fn has_origin(repo: &Path) -> bool {
    git(repo, &["remote", "get-url", "origin"]).is_ok()
}

/// The branch to index: `origin`'s default (from `origin/HEAD`), else `main` or `master`.
pub fn default_branch(repo: &Path) -> Result<String> {
    if let Ok(head) = git(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(b) = head.strip_prefix("origin/") {
            return Ok(b.to_string());
        }
    }
    let prefix = if has_origin(repo) {
        "refs/remotes/origin/"
    } else {
        "refs/heads/"
    };
    for b in ["main", "master"] {
        if git(
            repo,
            &["rev-parse", "--verify", "--quiet", &format!("{prefix}{b}")],
        )
        .is_ok()
        {
            return Ok(b.to_string());
        }
    }
    bail!("no main or master branch in {}", repo.display())
}

/// The remote's default branch and its commit, asked from the remote; also records it as `origin/HEAD`.
pub fn remote_default(repo: &Path, creds: &dyn Credentials) -> Result<(String, String)> {
    let auth = remote_url(repo).and_then(|u| auth_for(&u, creds));
    let out = git_auth(
        repo,
        &["ls-remote", "--symref", "origin", "HEAD"],
        auth.as_ref(),
    )?;
    let branch = out
        .lines()
        .find_map(|l| l.strip_prefix("ref: refs/heads/"))
        .and_then(|l| l.split_whitespace().next())
        .context("the remote does not say which branch is its default")?
        .to_string();
    let commit = out
        .lines()
        .find(|l| !l.starts_with("ref:") && l.ends_with("HEAD"))
        .and_then(|l| l.split_whitespace().next())
        .context("the remote's default branch has no commit")?
        .to_string();
    let _ = git(repo, &["remote", "set-head", "origin", &branch]);
    Ok((branch, commit))
}

/// The branch checked out in your own folder (for display).
pub fn current_branch(repo: &Path) -> Option<String> {
    git(repo, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()
}

pub fn remote_url(repo: &Path) -> Option<String> {
    git(repo, &["remote", "get-url", "origin"]).ok()
}

/// comindly's checkout of `repo`'s default branch, if one has been made.
pub fn checkout_dir(repo: &Path) -> Option<PathBuf> {
    worktree_dir(repo).ok().filter(|d| d.join(".git").exists())
}

fn worktree_dir(repo: &Path) -> Result<PathBuf> {
    let abs = std::fs::canonicalize(repo).with_context(|| format!("{}", repo.display()))?;
    let key = blake3::hash(abs.to_string_lossy().as_bytes()).to_hex();
    let home = std::env::var_os("HOME").context("HOME")?;
    Ok(PathBuf::from(home)
        .join(".cache/comindly/worktrees")
        .join(&key[..12])
        .join(name(repo)))
}

/// Fetch the remote's default branch and check it out in comindly's worktree, detached, so your
/// folder and branches are never touched. Offline, the last known default branch is used.
pub fn prepare(repo: &Path, creds: &dyn Credentials) -> Result<Checkout> {
    if !repo.join(".git").exists() {
        bail!("{} is not a git repository", repo.display());
    }
    let mut stale = None;
    let branch = if has_origin(repo) {
        match remote_default(repo, creds) {
            Ok((b, _)) => b,
            Err(e) => {
                stale = Some(format!("{e:#}"));
                default_branch(repo)?
            }
        }
    } else {
        default_branch(repo)?
    };
    // Also used for checking out: a partial clone fetches file contents then, with the same token.
    let auth = has_origin(repo)
        .then(|| git(repo, &["remote", "get-url", "origin"]).ok())
        .flatten()
        .and_then(|u| auth_for(&u, creds));
    let target = if has_origin(repo) {
        let target = format!("origin/{branch}");
        if let Err(e) = git_auth(
            repo,
            &["fetch", "--quiet", "origin", &branch],
            auth.as_ref(),
        ) {
            let why = format!("{e:#}");
            git(repo, &["rev-parse", "--verify", "--quiet", &target])
                .map_err(|_| e.context("and no earlier fetch to fall back to"))?;
            stale = Some(why);
        }
        target
    } else {
        branch.clone()
    };
    let dir = worktree_dir(repo)?;
    let _ = git(repo, &["worktree", "prune"]);
    if dir.join(".git").exists() {
        git_auth(
            &dir,
            &["checkout", "--quiet", "--detach", "--force", &target],
            auth.as_ref(),
        )?;
        git(&dir, &["clean", "-fdq"])?;
    } else {
        std::fs::create_dir_all(dir.parent().context("worktree parent")?)?;
        let d = dir.to_string_lossy();
        git_auth(
            repo,
            &[
                "worktree", "add", "--quiet", "--detach", "--force", &d, &target,
            ],
            auth.as_ref(),
        )?;
    }
    let commit = git(&dir, &["rev-parse", "--short", "HEAD"])?;
    Ok(Checkout {
        dir,
        target,
        commit,
        stale,
    })
}

/// Remove comindly's worktree for `repo` (your folder is untouched).
pub fn remove(repo: &Path) -> Result<()> {
    let dir = worktree_dir(repo)?;
    if dir.exists() {
        let d = dir.to_string_lossy();
        git(repo, &["worktree", "remove", "--force", &d])?;
    }
    Ok(())
}

/// Git repositories under `dir`, up to `depth` levels down (not inside other repositories).
pub fn scan(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries.into_iter().filter(|p| p.is_dir()) {
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden || path.ends_with("node_modules") {
            continue;
        }
        if path.join(".git").exists() {
            out.push(path);
        } else if depth > 1 {
            out.extend(scan(&path, depth - 1));
        }
    }
    out
}

/// Clone `url` into `parent/<name>` with your git credentials; returns the new folder.
/// comindly's own copy of `url` at `dest`: cloned once, without a checkout (indexing uses its worktree)
/// and without file contents until needed; later runs only fetch.
pub fn ensure_clone(url: &str, dest: &Path, creds: &dyn Credentials) -> Result<()> {
    if dest.join(".git").exists() || dest.join("HEAD").exists() {
        return Ok(());
    }
    let parent = dest.parent().context("clone folder")?;
    std::fs::create_dir_all(parent)?;
    let auth = auth_for(url, creds);
    let d = dest.to_string_lossy();
    let args = [
        "clone",
        "--quiet",
        "--filter=blob:none",
        "--no-checkout",
        url,
        &d,
    ];
    if let Err(e) = git_auth(parent, &args, auth.as_ref()) {
        let _ = std::fs::remove_dir_all(dest);
        if let Some(host) = https_host(url).filter(|_| auth_failed(&format!("{e:#}"))) {
            let username = token_user(&host).into();
            return Err(NeedsToken { host, username }.into());
        }
        return Err(e);
    }
    Ok(())
}

/// What your git credential helper holds for `host`.
pub fn helper_auth(host: &str) -> Option<Auth> {
    let input = format!("protocol=https\nhost={host}\n\n");
    let out = credential("fill", &input)?;
    let field = |k: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{k}=")))
            .map(str::to_string)
    };
    Some(Auth {
        username: field("username").unwrap_or_else(|| token_user(host).into()),
        token: field("password")?,
    })
}

/// Hand a token that worked to your git credential helper, as git does after a successful login.
pub fn approve(host: &str, auth: &Auth) {
    let input = format!(
        "protocol=https\nhost={host}\nusername={}\npassword={}\n\n",
        auth.username, auth.token
    );
    let _ = credential("approve", &input);
}

fn credential(action: &str, input: &str) -> Option<String> {
    use std::io::Write;
    let mut child = Command::new("git")
        .args(["credential", action])
        .env("GIT_TERMINAL_PROMPT", "0")
        // Only read what is stored; Git Credential Manager would otherwise open a login window.
        .env("GCM_INTERACTIVE", "never")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_and_token_users() {
        assert_eq!(
            https_host("https://user@gitlab.example.com:8443/g/r.git").as_deref(),
            Some("gitlab.example.com")
        );
        assert_eq!(https_host("git@github.com:o/r.git"), None);
        assert_eq!(token_user("github.com"), "x-access-token");
        assert_eq!(token_user("gitlab.com"), "oauth2");
        assert!(auth_failed(
            "fatal: could not read Username for 'https://gitlab.com': terminal prompts disabled"
        ));
        assert!(!auth_failed("fatal: repository 'x' does not exist"));
    }

    #[test]
    fn worktree_follows_main_and_leaves_the_folder_alone() {
        let base = std::env::temp_dir().join(format!("comindly-wt-{}", std::process::id()));
        let origin = base.join("origin");
        let mine = base.join("mine");
        std::fs::create_dir_all(&origin).unwrap();
        let g = |d: &Path, a: &[&str]| git(d, a).unwrap();
        g(&origin, &["init", "--quiet", "-b", "main"]);
        g(&origin, &["config", "user.email", "t@t"]);
        g(&origin, &["config", "user.name", "t"]);
        std::fs::write(origin.join("a.py"), "x = 1\n").unwrap();
        g(&origin, &["add", "."]);
        g(&origin, &["commit", "--quiet", "-m", "one"]);
        g(
            &base,
            &["clone", "--quiet", origin.to_str().unwrap(), "mine"],
        );
        g(&mine, &["checkout", "--quiet", "-b", "feature"]);
        std::fs::write(mine.join("wip.py"), "draft\n").unwrap();

        std::fs::write(origin.join("b.py"), "y = 2\n").unwrap();
        g(&origin, &["add", "."]);
        g(&origin, &["commit", "--quiet", "-m", "two"]);

        let c = prepare(&mine, &NoCredentials).unwrap();
        assert_eq!(c.target, "origin/main");
        assert!(
            c.dir.join("b.py").exists(),
            "the worktree has the newest main"
        );
        assert!(!c.dir.join("wip.py").exists());
        assert_eq!(current_branch(&mine).as_deref(), Some("feature"));
        assert!(mine.join("wip.py").exists(), "your folder is untouched");
        g(&mine, &["checkout", "--quiet", "main"]);
        assert!(
            prepare(&mine, &NoCredentials).is_ok(),
            "a second run reuses the worktree"
        );
        remove(&mine).unwrap();
        assert!(!c.dir.exists());
        assert_eq!(scan(&base, 1).len(), 2);
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn indexes_the_remotes_default_branch_whatever_its_name() {
        let base = std::env::temp_dir().join(format!("comindly-trunk-{}", std::process::id()));
        let origin = base.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        let g = |d: &Path, a: &[&str]| git(d, a).unwrap();
        g(&origin, &["init", "--quiet", "-b", "trunk"]);
        g(&origin, &["config", "user.email", "t@t"]);
        g(&origin, &["config", "user.name", "t"]);
        std::fs::write(origin.join("a.py"), "x = 1\n").unwrap();
        g(&origin, &["add", "."]);
        g(&origin, &["commit", "--quiet", "-m", "one"]);
        let url = format!("file://{}", origin.display());
        let mine = base.join("managed").join("origin");
        ensure_clone(&url, &mine, &NoCredentials).unwrap();
        ensure_clone(&url, &mine, &NoCredentials).unwrap();
        let (branch, commit) = remote_default(&mine, &NoCredentials).unwrap();
        assert_eq!(branch, "trunk");
        let c = prepare(&mine, &NoCredentials).unwrap();
        assert_eq!(c.target, "origin/trunk");
        assert_eq!(c.commit, commit[..c.commit.len()]);
        assert!(c.dir.join("a.py").exists());
        std::fs::remove_dir_all(&base).ok();
        std::fs::remove_dir_all(c.dir.parent().unwrap()).ok();
    }
}
