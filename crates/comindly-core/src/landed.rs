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
    let closed = close_discussions(ctx, &remotes, &client, &mut asked, &mut failed).await;
    if closed > 0 {
        ctx.report.note(format!("discussions: {closed} closed"));
    }
    if let Some(e) = failed {
        ctx.report.warn(format!("evidence lookup: {e}"));
    }
}

/// Close each open discussion whose request merged or was closed after it opened, or that had no
/// post for `discussion_inactive_days`; the number closed.
async fn close_discussions(
    ctx: &IndexContext,
    remotes: &HashMap<&str, &str>,
    client: &reqwest::Client,
    asked: &mut usize,
    failed: &mut Option<String>,
) -> usize {
    use crate::discussion::{Closed, Discussions};
    let Ok(store) = Discussions::open(&ctx.root) else {
        return 0;
    };
    let all = match store.all().await {
        Ok(all) => all,
        Err(e) => {
            *failed = Some(format!("{e:#}"));
            return 0;
        }
    };
    let now = chrono::Utc::now();
    let quiet = ctx.settings.discussion_inactive_days as i64;
    let mut closed = 0;
    for t in all.iter().filter(|t| t.closed.is_none()) {
        let d = &t.discussion;
        let opened = time(&d.created).unwrap_or(i64::MAX);
        let mut ended = None;
        let project = remotes.get(d.repo.as_str()).and_then(|r| project(r, ctx));
        if let Some(project) = project.filter(|_| *asked < BUDGET) {
            // The request a post cited last, else the latest one from the discussion's branch.
            let cited = t.posts.iter().rev().find_map(|p| {
                p.evidence.iter().rev().find_map(|e| match reference(e) {
                    Some(r @ Ref::Request(_, n)) if r.names(&d.repo) => Some(n),
                    _ => None,
                })
            });
            let found = match (cited, &d.branch) {
                (Some(n), _) => Some(project.request(client, n).await),
                (None, Some(b)) => Some(project.on_branch(client, b).await),
                (None, None) => None,
            };
            if found.is_some() {
                *asked += 1;
            }
            match found {
                Some(Ok(Some((n, reason, at)))) if at >= opened => {
                    let request = match project.provider {
                        Provider::GitLab => format!("mr:!{n}"),
                        Provider::GitHub => format!("pr:#{n}"),
                    };
                    ended = Some(Closed {
                        reason,
                        request: Some(request),
                        by: None,
                        note: None,
                        at: rfc3339(at),
                    });
                }
                Some(Err(e)) => *failed = Some(format!("{e:#}")),
                _ => {}
            }
        }
        if let Some(c) = ended.or_else(|| inactive(t, now.timestamp(), quiet)) {
            match store.close(&d.id, &c).await {
                Ok(()) => closed += 1,
                Err(e) => *failed = Some(format!("{e:#}")),
            }
        }
    }
    closed
}

/// How `t` closes as inactive at `now` when nobody posted for `days` days (0 never).
fn inactive(
    t: &crate::discussion::Transcript,
    now: i64,
    days: i64,
) -> Option<crate::discussion::Closed> {
    let last = time(t.last())?;
    (days > 0 && now - last > days * 86_400).then(|| crate::discussion::Closed {
        reason: crate::discussion::CloseReason::Inactive,
        request: None,
        by: None,
        note: None,
        at: rfc3339(now),
    })
}

/// Seconds since the epoch of an RFC 3339 time.
fn time(t: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(t)
        .ok()
        .map(|t| t.timestamp())
}

fn rfc3339(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Whether a request merged or was closed, and when; `None` while it is open.
fn ended(
    provider: Provider,
    req: &serde_json::Value,
) -> Option<(crate::discussion::CloseReason, i64)> {
    use crate::discussion::CloseReason;
    let at = |k: &str| req[k].as_str().and_then(time);
    let merged = match provider {
        Provider::GitLab => req["state"] == "merged",
        Provider::GitHub => !req["merged_at"].is_null(),
    };
    if merged {
        return Some((CloseReason::Merged, at("merged_at")?));
    }
    (req["state"] == "closed").then(|| Some((CloseReason::Closed, at("closed_at")?)))?
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
    owner: String,
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
        owner: src.owner,
    })
}

impl Project {
    async fn get(
        &self,
        client: &reqwest::Client,
        path: &str,
    ) -> anyhow::Result<Option<serde_json::Value>> {
        self.get_query(client, path, &[]).await
    }

    async fn get_query(
        &self,
        client: &reqwest::Client,
        path: &str,
        query: &[(&str, &str)],
    ) -> anyhow::Result<Option<serde_json::Value>> {
        let req = client.get(format!("{}{path}", self.api)).query(query);
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

    /// Request `n`, if it merged or was closed: its number, how it ended and when.
    async fn request(
        &self,
        client: &reqwest::Client,
        n: u64,
    ) -> anyhow::Result<Option<(u64, crate::discussion::CloseReason, i64)>> {
        let path = match self.provider {
            Provider::GitLab => format!("/merge_requests/{n}"),
            Provider::GitHub => format!("/pulls/{n}"),
        };
        Ok(self
            .get(client, &path)
            .await?
            .and_then(|r| ended(self.provider, &r))
            .map(|(reason, at)| (n, reason, at)))
    }

    /// The newest request from `branch` in this repository, if it merged or was closed; an open
    /// newer one means the work goes on, so the discussion stays open. Newest by creation, so a
    /// comment on an old request does not make it the latest.
    async fn on_branch(
        &self,
        client: &reqwest::Client,
        branch: &str,
    ) -> anyhow::Result<Option<(u64, crate::discussion::CloseReason, i64)>> {
        let (path, head, number) = match self.provider {
            Provider::GitLab => ("/merge_requests", branch.to_string(), "iid"),
            Provider::GitHub => ("/pulls", format!("{}:{branch}", self.owner), "number"),
        };
        let query: &[(&str, &str)] = match self.provider {
            Provider::GitLab => &[
                ("source_branch", &head),
                ("state", "all"),
                ("order_by", "created_at"),
                ("per_page", "1"),
            ],
            Provider::GitHub => &[
                ("head", &head),
                ("state", "all"),
                ("sort", "created"),
                ("direction", "desc"),
                ("per_page", "1"),
            ],
        };
        let latest = match self.get_query(client, path, query).await? {
            Some(serde_json::Value::Array(v)) => v.into_iter().next(),
            _ => None,
        };
        Ok(latest.and_then(|r| {
            let (reason, at) = ended(self.provider, &r)?;
            Some((r[number].as_u64()?, reason, at))
        }))
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
    fn a_request_ends_merged_or_closed() {
        use crate::discussion::CloseReason;
        use serde_json::json;
        let gh_merged = json!({"state": "closed", "merged_at": "2026-11-17T10:44:01Z", "closed_at": "2026-11-17T10:44:01Z"});
        let gh_closed =
            json!({"state": "closed", "merged_at": null, "closed_at": "2026-11-17T11:00:00Z"});
        let gh_open = json!({"state": "open", "merged_at": null, "closed_at": null});
        let gl_merged = json!({"state": "merged", "merged_at": "2026-11-17T10:44:01Z"});
        let gl_closed = json!({"state": "closed", "closed_at": "2026-11-17T11:00:00Z"});
        let gl_open = json!({"state": "opened"});
        let at = time("2026-11-17T10:44:01Z").unwrap();
        assert_eq!(
            ended(Provider::GitHub, &gh_merged),
            Some((CloseReason::Merged, at))
        );
        assert_eq!(
            ended(Provider::GitHub, &gh_closed).map(|e| e.0),
            Some(CloseReason::Closed)
        );
        assert_eq!(ended(Provider::GitHub, &gh_open), None);
        assert_eq!(
            ended(Provider::GitLab, &gl_merged),
            Some((CloseReason::Merged, at))
        );
        assert_eq!(
            ended(Provider::GitLab, &gl_closed).map(|e| e.0),
            Some(CloseReason::Closed)
        );
        assert_eq!(ended(Provider::GitLab, &gl_open), None);
    }

    #[test]
    fn a_quiet_discussion_closes_as_inactive() {
        use crate::discussion::{CloseReason, Discussion, Transcript};
        use crate::ledger::Provenance;
        let mut d = Discussion::new(
            "r",
            Some("LIN-1"),
            None,
            "t",
            Provenance::agent("t", None, &Default::default()),
        );
        d.created = "2026-10-01T00:00:00Z".into();
        let t = Transcript {
            discussion: d,
            posts: vec![],
            closed: None,
        };
        let day = 86_400;
        let opened = time("2026-10-01T00:00:00Z").unwrap();
        assert!(
            inactive(&t, opened + 30 * day, 30).is_none(),
            "not before the period ends"
        );
        let c = inactive(&t, opened + 31 * day, 30).unwrap();
        assert_eq!((c.reason, c.request), (CloseReason::Inactive, None));
        assert!(
            inactive(&t, opened + 400 * day, 0).is_none(),
            "0 never closes"
        );
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
