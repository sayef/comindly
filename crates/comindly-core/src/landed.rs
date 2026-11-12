//! Where the merge and pull requests and branch commits cited as evidence landed on the default branch
//! (GitLab and GitHub).

use std::collections::{BTreeMap, HashMap};

use crate::context::IndexContext;
use crate::hosting::Provider;
use crate::index::{IndexedRepo, RepoLogs};

/// Lookups per indexing run; the rest wait for the next run.
const BUDGET: usize = 200;

/// Carry earlier answers forward and look up the evidence on records that is not on the default branch yet.
pub async fn resolve(ctx: &IndexContext, heads: &[IndexedRepo], logs: &mut RepoLogs) {
    if let Ok(old) = crate::index::read_repo_logs(&ctx.graph()).await {
        for (repo, log) in old {
            if let Some(now) = logs.get_mut(&repo) {
                now.landed = log.landed;
            }
        }
    }
    let Ok(ledger) = crate::ledger::Ledger::open(&ctx.root) else {
        return;
    };
    let records = ledger.all().await.unwrap_or_default();
    // Each cited request per anchored repository, with the earliest code a record citing it saw.
    let mut wanted: BTreeMap<(String, String), Option<i64>> = BTreeMap::new();
    for r in &records {
        for (ev, re) in r.evidence.iter().filter_map(|e| Some((e, reference(e)?))) {
            for a in r.anchors.iter().filter(|a| re.names(&a.repo)) {
                let saw = logs.get(&a.repo).and_then(|l| {
                    let c = a.commit.as_deref()?;
                    l.commits.iter().find(|x| x.sha == c).map(|x| x.time)
                });
                let slot = wanted.entry((a.repo.clone(), ev.clone())).or_insert(saw);
                *slot = match (*slot, saw) {
                    (Some(x), Some(y)) => Some(x.min(y)),
                    (x, y) => x.or(y),
                };
            }
        }
    }
    let remotes: HashMap<&str, &str> = heads
        .iter()
        .filter_map(|h| Some((h.name.as_str(), h.remote.as_deref()?)))
        .collect();
    let client = match reqwest::Client::builder()
        .user_agent(concat!("comindly/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let (mut asked, mut found, mut failed) = (0, 0, None);
    for ((repo, ev), saw) in wanted {
        let Some(log) = logs.get_mut(&repo) else {
            continue;
        };
        let on_main = matches!(reference(&ev), Some(Ref::Commit(_, sha)) if log.commits.iter().any(|c| c.sha.starts_with(sha)));
        if on_main || log.landed.contains_key(&ev) || asked >= BUDGET {
            continue;
        }
        let Some(project) = remotes.get(repo.as_str()).and_then(|r| project(r, ctx)) else {
            continue;
        };
        asked += 1;
        match project.landed(&client, &ev).await {
            // A request merged before the record's code was indexed is another one with that number.
            Ok(Some((_, at))) if saw.is_some_and(|s| at < s) => {
                log.landed.insert(ev, Vec::new());
            }
            Ok(Some((shas, _))) if !shas.is_empty() => {
                found += 1;
                log.landed.insert(ev, shas);
            }
            Ok(_) => {}
            Err(e) => failed = Some(format!("{e:#}")),
        }
    }
    if found > 0 {
        ctx.report.note(format!(
            "evidence: {found} merge request(s) or branch commit(s) followed to the default branch"
        ));
    }
    if let Some(e) = failed {
        ctx.report.warn(format!("evidence lookup: {e}"));
    }
}

/// A merge or pull request (`mr:!N`, `pr:#N`) or commit, optionally naming its repository:
/// `mr:repo!N`, `pr:owner/repo#N`, `commit:repo@sha`.
pub(crate) enum Ref<'a> {
    Request(Option<&'a str>, u64),
    Commit(Option<&'a str>, &'a str),
}

impl Ref<'_> {
    /// Whether this evidence can be about `repo` (unqualified evidence is about every anchored one).
    pub(crate) fn names(&self, repo: &str) -> bool {
        let (Ref::Request(q, _) | Ref::Commit(q, _)) = self;
        q.is_none_or(|q| {
            let q = q.trim_end_matches(".git");
            q.eq_ignore_ascii_case(repo)
                || q.rsplit('/')
                    .next()
                    .is_some_and(|l| l.eq_ignore_ascii_case(repo))
        })
    }
}

fn qualifier(q: &str) -> Option<Option<&str>> {
    let q = q.trim();
    if q.is_empty() {
        return Some(None);
    }
    q.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
        .then_some(Some(q))
}

pub(crate) fn reference(ev: &str) -> Option<Ref<'_>> {
    let ev = ev.trim();
    for (prefix, sep) in [("mr:", '!'), ("pr:", '#')] {
        if let Some(rest) = ev.strip_prefix(prefix) {
            let (q, n) = rest.rsplit_once(sep).unwrap_or(("", rest));
            return Some(Ref::Request(qualifier(q)?, n.trim().parse().ok()?));
        }
    }
    let rest = ev.strip_prefix("commit:")?.trim();
    let (q, sha) = rest.rsplit_once('@').unwrap_or(("", rest));
    let sha = sha.trim();
    (sha.len() >= 7 && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(Ref::Commit(qualifier(q)?, sha))
}

struct Project {
    provider: Provider,
    api: String,
    token: String,
}

fn project(remote: &str, ctx: &IndexContext) -> Option<Project> {
    let src = crate::hosting::parse(remote).ok()?;
    let name = src.repo.clone()?;
    let token = ctx.git.token(&src.host)?;
    let api = match src.provider {
        Provider::GitLab => format!(
            "https://{}/api/v4/projects/{}",
            src.host,
            format!("{}/{name}", src.owner).replace('/', "%2F")
        ),
        Provider::GitHub if src.host == "github.com" => {
            format!("https://api.github.com/repos/{}/{name}", src.owner)
        }
        Provider::GitHub => format!("https://{}/api/v3/repos/{}/{name}", src.host, src.owner),
    };
    Some(Project {
        provider: src.provider,
        api,
        token,
    })
}

impl Project {
    async fn get(
        &self,
        client: &reqwest::Client,
        path: &str,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let req = client.get(format!("{}{path}", self.api));
        let req = match self.provider {
            Provider::GitLab => req.header("PRIVATE-TOKEN", &self.token),
            Provider::GitHub => req
                .bearer_auth(&self.token)
                .header("Accept", "application/vnd.github+json"),
        };
        let resp = req.send().await?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        anyhow::ensure!(
            resp.status().is_success(),
            "{} answered {}",
            self.api.split('/').nth(2).unwrap_or_default(),
            resp.status()
        );
        Ok(Some(resp.json().await?))
    }

    async fn list(
        &self,
        client: &reqwest::Client,
        path: &str,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        Ok(match self.get(client, path).await? {
            Some(serde_json::Value::Array(v)) => v,
            _ => Vec::new(),
        })
    }

    /// The default-branch commits a merged request, or the request a branch commit belongs to, became,
    /// and when it was merged; `None` while it is not merged.
    async fn landed(
        &self,
        client: &reqwest::Client,
        ev: &str,
    ) -> anyhow::Result<Option<(Vec<String>, i64)>> {
        let gitlab = self.provider == Provider::GitLab;
        let number = match reference(ev) {
            Some(Ref::Request(_, n)) => n,
            Some(Ref::Commit(_, sha)) => {
                let path = if gitlab {
                    format!("/repository/commits/{sha}/merge_requests")
                } else {
                    format!("/commits/{sha}/pulls")
                };
                let merged = self.list(client, &path).await?.into_iter().find(|m| {
                    if gitlab {
                        m["state"] == "merged"
                    } else {
                        !m["merged_at"].is_null()
                    }
                });
                match merged.and_then(|m| m[if gitlab { "iid" } else { "number" }].as_u64()) {
                    Some(n) => n,
                    None => return Ok(None),
                }
            }
            None => return Ok(None),
        };
        let path = if gitlab {
            format!("/merge_requests/{number}")
        } else {
            format!("/pulls/{number}")
        };
        let Some(req) = self.get(client, &path).await? else {
            return Ok(None);
        };
        let merged = req["merged_at"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.timestamp());
        let Some(at) = merged.filter(|_| !gitlab || req["state"] == "merged") else {
            return Ok(None);
        };
        // GitHub's merge_commit_sha is the merge or squash commit, or the last rebased commit.
        let keys: &[&str] = if gitlab {
            &["squash_commit_sha", "merge_commit_sha"]
        } else {
            &["merge_commit_sha"]
        };
        let mut out: Vec<String> = keys
            .iter()
            .find_map(|k| req[*k].as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();
        if gitlab && out.is_empty() {
            // Fast-forward without squash: the MR's own commits are on the default branch.
            out = self
                .list(
                    client,
                    &format!("/merge_requests/{number}/commits?per_page=100"),
                )
                .await?
                .iter()
                .filter_map(|c| c["id"].as_str().map(str::to_string))
                .collect();
        }
        Ok(Some((out, at)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_merge_requests_and_commit_shas_are_looked_up() {
        assert!(matches!(
            reference("mr:!476"),
            Some(Ref::Request(None, 476))
        ));
        assert!(matches!(reference("pr:#12"), Some(Ref::Request(None, 12))));
        assert!(matches!(reference("mr:476"), Some(Ref::Request(None, 476))));
        assert!(matches!(
            reference("commit:51a59d4"),
            Some(Ref::Commit(None, "51a59d4"))
        ));
        assert!(reference("commit:abc").is_none());
        assert!(reference("test:t::retry").is_none());
        assert!(reference("dev logs 2026-11-11").is_none());
        assert!(reference("mr:see the thread").is_none());
    }

    #[test]
    fn evidence_can_name_its_repository() {
        assert!(matches!(
            reference("mr:billing-api!472"),
            Some(Ref::Request(Some("billing-api"), 472))
        ));
        assert!(matches!(
            reference("pr:acme/web#12"),
            Some(Ref::Request(Some("acme/web"), 12))
        ));
        assert!(matches!(
            reference("commit:web@51a59d4"),
            Some(Ref::Commit(Some("web"), "51a59d4"))
        ));
        let r = reference("mr:group/billing-api!472").unwrap();
        assert!(r.names("billing-api") && !r.names("web"));
        assert!(reference("mr:!1").unwrap().names("web"));
        assert!(reference("mr:a b!1").is_none());
    }
}
