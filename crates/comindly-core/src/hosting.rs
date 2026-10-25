//! Lists the repositories of a GitHub organisation or user, or a GitLab group (with subgroups), so
//! they can be picked and cloned. Works with github.com, GitHub Enterprise, gitlab.com and self-hosted GitLab.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::worktree::{Auth, NeedsToken};

/// Where the repositories live: a provider, its host and the owner or group path.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub provider: Provider,
    pub host: String,
    /// Organisation or user on GitHub; group or project path (`team/platform`) on GitLab.
    pub owner: String,
    /// The repository, when the URL names one (`github.com/acme/orders`, `….git`, `git@…`).
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    GitHub,
    GitLab,
}

/// A repository on the host.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRepo {
    pub name: String,
    /// `owner/name` or `group/sub/name`.
    pub full_path: String,
    pub clone_url: String,
    pub description: Option<String>,
    pub default_branch: Option<String>,
    pub archived: bool,
    pub fork: bool,
    pub last_activity: Option<String>,
    /// Size in KiB, when the host reports it.
    pub size_kib: Option<u64>,
}

/// Read `https://github.com/acme`, `gitlab.com/acme/platform` or `https://git.example.com/group`.
pub fn parse(url: &str) -> Result<Source> {
    let url = url.trim().trim_end_matches('/');
    let clone_url = url.ends_with(".git") || url.starts_with("git@") || url.starts_with("ssh://");
    let rest = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("git@");
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let (host, path) = rest
        .split_once(['/', ':'])
        .context("give the organisation, group or repository, e.g. github.com/acme")?;
    let path = path.trim_end_matches(".git").trim_matches('/');
    let path = path.split("/-/").next().unwrap_or(path);
    anyhow::ensure!(
        !path.is_empty(),
        "give the organisation, group or repository after the host"
    );
    let host = host.to_lowercase();
    let provider = if host == "github.com" || host.contains("github") {
        Provider::GitHub
    } else {
        Provider::GitLab
    };
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let (owner, repo) = match provider {
        // github.com/orgs/acme/repositories is the organisation; github.com/acme/orders a repository.
        Provider::GitHub if parts.first() == Some(&"orgs") => {
            (parts.get(1).unwrap_or(&"").to_string(), None)
        }
        Provider::GitHub => (parts[0].to_string(), parts.get(1).map(|r| r.to_string())),
        Provider::GitLab if clone_url && parts.len() >= 2 => {
            let (group, name) = path.rsplit_once('/').unwrap_or_default();
            (group.to_string(), Some(name.to_string()))
        }
        Provider::GitLab => (path.to_string(), None),
    };
    anyhow::ensure!(
        !owner.is_empty(),
        "give the organisation, group or repository after the host"
    );
    Ok(Source {
        provider,
        host,
        owner,
        repo,
    })
}

/// Every repository of `source`, newest activity first.
pub async fn list(source: &Source, auth: Option<&Auth>) -> Result<Vec<RemoteRepo>> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("comindly/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut repos = match source.provider {
        Provider::GitHub => match &source.repo {
            Some(name) => {
                let api = github_api(&source.host);
                let url = format!("{api}/repos/{}/{name}", source.owner);
                let one = one(&client, &url, auth, false, &source.host)
                    .await?
                    .with_context(|| {
                        format!("no repository {}/{name} on {}", source.owner, source.host)
                    })?;
                github_repo(&one).into_iter().collect()
            }
            None => github(&client, source, auth).await?,
        },
        Provider::GitLab => {
            // `a/b` is a project or a subgroup; ask for the project first.
            let path = match &source.repo {
                Some(name) => format!("{}/{name}", source.owner),
                None => source.owner.clone(),
            };
            let url = format!(
                "https://{}/api/v4/projects/{}",
                source.host,
                path.replace('/', "%2F")
            );
            let project = if path.contains('/') {
                one(&client, &url, auth, true, &source.host).await?
            } else {
                None
            };
            match project {
                Some(p) => gitlab_repo(&p).into_iter().collect(),
                None if source.repo.is_some() => bail!("no repository {path} on {}", source.host),
                None => gitlab(&client, source, auth).await?,
            }
        }
    };
    repos.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    Ok(repos)
}

/// One object, or `None` when it is not found.
async fn one(
    client: &reqwest::Client,
    url: &str,
    auth: Option<&Auth>,
    gitlab: bool,
    host: &str,
) -> Result<Option<serde_json::Value>> {
    let mut req = client.get(url);
    if let Some(a) = auth {
        req = if gitlab {
            req.header("PRIVATE-TOKEN", &a.token)
        } else {
            req.bearer_auth(&a.token)
        };
    }
    let resp = req.send().await.with_context(|| format!("reach {host}"))?;
    match resp.status().as_u16() {
        404 => Ok(None),
        401 | 403 => Err(NeedsToken {
            host: host.to_string(),
            username: crate::worktree::token_user(host).into(),
        }
        .into()),
        s if !resp.status().is_success() => bail!("{host} answered {s}"),
        _ => Ok(Some(resp.json().await?)),
    }
}

fn github_api(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_string()
    } else {
        format!("https://{host}/api/v3")
    }
}

/// Fetch every page of a listing; `None` when the first page is not found.
async fn pages(
    client: &reqwest::Client,
    url: &str,
    auth: Option<&Auth>,
    gitlab: bool,
    host: &str,
) -> Result<Option<Vec<serde_json::Value>>> {
    let mut out = Vec::new();
    for page in 1..=50 {
        let sep = if url.contains('?') { '&' } else { '?' };
        let mut req = client.get(format!("{url}{sep}per_page=100&page={page}"));
        if let Some(a) = auth {
            req = if gitlab {
                req.header("PRIVATE-TOKEN", &a.token)
            } else {
                req.bearer_auth(&a.token)
            };
        }
        let resp = req.send().await.with_context(|| format!("reach {host}"))?;
        let status = resp.status().as_u16();
        if status == 404 && page == 1 {
            return Ok(None);
        }
        if status == 401 || status == 403 {
            return Err(NeedsToken {
                host: host.to_string(),
                username: crate::worktree::token_user(host).into(),
            }
            .into());
        }
        if !resp.status().is_success() {
            bail!("{host} answered {status}");
        }
        let items: Vec<serde_json::Value> = resp.json().await?;
        let done = items.len() < 100;
        out.extend(items);
        if done {
            break;
        }
    }
    Ok(Some(out))
}

async fn github(
    client: &reqwest::Client,
    s: &Source,
    auth: Option<&Auth>,
) -> Result<Vec<RemoteRepo>> {
    let api = github_api(&s.host);
    let org = format!("{api}/orgs/{}/repos?type=all", s.owner);
    let items = match pages(client, &org, auth, false, &s.host).await? {
        Some(items) => items,
        None => {
            let user = format!("{api}/users/{}/repos?type=owner", s.owner);
            pages(client, &user, auth, false, &s.host)
                .await?
                .with_context(|| format!("no organisation or user {} on {}", s.owner, s.host))?
        }
    };
    Ok(items.iter().filter_map(github_repo).collect())
}

fn github_repo(v: &serde_json::Value) -> Option<RemoteRepo> {
    #[derive(Deserialize)]
    struct R {
        name: String,
        full_name: String,
        clone_url: String,
        description: Option<String>,
        default_branch: Option<String>,
        #[serde(default)]
        archived: bool,
        #[serde(default)]
        fork: bool,
        pushed_at: Option<String>,
        size: Option<u64>,
    }
    let r: R = serde_json::from_value(v.clone()).ok()?;
    Some(RemoteRepo {
        name: r.name,
        full_path: r.full_name,
        clone_url: r.clone_url,
        description: r.description.filter(|d| !d.is_empty()),
        default_branch: r.default_branch,
        archived: r.archived,
        fork: r.fork,
        last_activity: r.pushed_at,
        size_kib: r.size,
    })
}

async fn gitlab(
    client: &reqwest::Client,
    s: &Source,
    auth: Option<&Auth>,
) -> Result<Vec<RemoteRepo>> {
    let id = s.owner.replace('/', "%2F");
    let group = format!(
        "https://{}/api/v4/groups/{id}/projects?include_subgroups=true&order_by=last_activity_at",
        s.host
    );
    let items = match pages(client, &group, auth, true, &s.host).await? {
        Some(items) => items,
        None => {
            let user = format!("https://{}/api/v4/users/{id}/projects", s.host);
            pages(client, &user, auth, true, &s.host)
                .await?
                .with_context(|| format!("no group or user {} on {}", s.owner, s.host))?
        }
    };
    Ok(items.iter().filter_map(gitlab_repo).collect())
}

fn gitlab_repo(v: &serde_json::Value) -> Option<RemoteRepo> {
    #[derive(Deserialize)]
    struct R {
        path: String,
        path_with_namespace: String,
        http_url_to_repo: String,
        description: Option<String>,
        default_branch: Option<String>,
        #[serde(default)]
        archived: bool,
        forked_from_project: Option<serde_json::Value>,
        last_activity_at: Option<String>,
    }
    let r: R = serde_json::from_value(v.clone()).ok()?;
    Some(RemoteRepo {
        name: r.path,
        full_path: r.path_with_namespace,
        clone_url: r.http_url_to_repo,
        description: r.description.filter(|d| !d.is_empty()),
        default_branch: r.default_branch,
        archived: r.archived,
        fork: r.forked_from_project.is_some(),
        last_activity: r.last_activity_at,
        size_kib: None,
    })
}

/// Where comindly keeps its own clone of `clone_url`: `<data>/repos/<host>/<path>`.
pub fn managed_path(data_dir: &std::path::Path, clone_url: &str) -> Result<std::path::PathBuf> {
    let rest = clone_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("git@");
    let rest = rest.split_once('@').map_or(rest, |(_, r)| r);
    let (host, path) = rest
        .split_once(['/', ':'])
        .context("cannot tell the host from the URL")?;
    let path = path.trim_end_matches(".git").trim_matches('/');
    anyhow::ensure!(
        !host.is_empty() && !path.is_empty() && !path.contains(".."),
        "cannot tell the repository from {clone_url}"
    );
    Ok(data_dir
        .join("repos")
        .join(host.split(':').next().unwrap_or(host))
        .join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: &str = "/data/comindly";

    #[test]
    fn reads_org_and_group_urls() {
        let s = parse("https://github.com/acme").unwrap();
        assert_eq!(
            (s.provider, s.host.as_str(), s.owner.as_str()),
            (Provider::GitHub, "github.com", "acme")
        );
        assert_eq!(
            parse("github.com/orgs/acme/repositories").unwrap().owner,
            "acme"
        );
        let g = parse("https://gitlab.com/acme/platform/-/tree/main").unwrap();
        assert_eq!(
            (g.provider, g.owner.as_str()),
            (Provider::GitLab, "acme/platform")
        );
        let own = parse("git.example.com/team/").unwrap();
        assert_eq!(
            (own.provider, own.host.as_str(), own.owner.as_str()),
            (Provider::GitLab, "git.example.com", "team")
        );
        assert!(parse("github.com").is_err());
        let r = parse("https://github.com/acme/orders").unwrap();
        assert_eq!(
            (r.owner.as_str(), r.repo.as_deref()),
            ("acme", Some("orders"))
        );
        let ssh = parse("git@gitlab.com:acme/platform/payments.git").unwrap();
        assert_eq!(
            (ssh.host.as_str(), ssh.owner.as_str(), ssh.repo.as_deref()),
            ("gitlab.com", "acme/platform", Some("payments"))
        );
        assert_eq!(parse("gitlab.com/acme/platform").unwrap().repo, None);
    }

    #[test]
    fn reads_api_answers() {
        let gh = serde_json::json!({"name": "orders", "full_name": "acme/orders", "clone_url": "https://github.com/acme/orders.git",
            "description": "", "default_branch": "trunk", "archived": false, "fork": false, "pushed_at": "2026-10-01T00:00:00Z", "size": 2048});
        let r = github_repo(&gh).unwrap();
        assert_eq!(
            (
                r.full_path.as_str(),
                r.default_branch.as_deref(),
                r.description
            ),
            ("acme/orders", Some("trunk"), None)
        );
        let gl = serde_json::json!({"path": "payments", "path_with_namespace": "acme/platform/payments",
            "http_url_to_repo": "https://gitlab.com/acme/platform/payments.git", "description": "Charges", "default_branch": "master",
            "archived": true, "last_activity_at": "2026-09-01T00:00:00Z"});
        let r = gitlab_repo(&gl).unwrap();
        assert_eq!(
            (r.name.as_str(), r.archived, r.fork),
            ("payments", true, false)
        );
    }

    #[test]
    fn managed_paths_follow_host_and_group() {
        let p = managed_path(
            std::path::Path::new(DATA),
            "https://gitlab.com/acme/platform/payments.git",
        )
        .unwrap();
        assert!(
            p.ends_with("repos/gitlab.com/acme/platform/payments"),
            "{}",
            p.display()
        );
        let s = managed_path(std::path::Path::new(DATA), "git@github.com:acme/orders.git").unwrap();
        assert!(
            s.ends_with("repos/github.com/acme/orders"),
            "{}",
            s.display()
        );
        assert!(managed_path(std::path::Path::new(DATA), "https://github.com/../etc").is_err());
    }
}
