//! Commands answered by a comindly server: the same answers a local run computes, printed the same
//! way, and indexing as a job on the server that this terminal follows.

use std::process::ExitCode;

use comindly::client::Client;
use serde_json::json;

use crate::{Cmd, ConventionsWhat};

/// The server to ask: `--local` never, else `--server` (or `COMINDLY_SERVER`), else the one
/// `comindly login` signed in to. `COMINDLY_TOKEN` stands in for a sign-in, e.g. in CI.
pub fn client(server: Option<&str>, local: bool) -> Option<Client> {
    if local {
        return None;
    }
    let saved = comindly::credentials::login();
    let url = server
        .map(str::to_string)
        .or_else(|| comindly::config::env("COMINDLY_SERVER").filter(|u| !u.is_empty()));
    match url {
        Some(url) => {
            let url = url.trim_end_matches('/').to_string();
            let token = comindly::config::env("COMINDLY_TOKEN")
                .filter(|t| !t.is_empty())
                .or_else(|| saved.filter(|l| l.url == url).map(|l| l.token));
            Some(Client::new(&url, token))
        }
        None => saved.map(|l| Client::new(&l.url, Some(l.token))),
    }
}

/// Doctor's line for the server: who this machine is signed in as, and whether that still works.
pub fn sign_in_check(c: &Client) -> comindly::doctor::Check {
    let me = comindly::rt::block_on(c.get::<serde_json::Value>("/api/v1/auth/me", &[]));
    let (ok, message) = match me {
        Ok(m) if m["signed_in"] == true => (
            true,
            format!(
                "signed in to {} as {} ({})",
                c.base(),
                m["email"].as_str().unwrap_or("?"),
                m["role"].as_str().unwrap_or("?")
            ),
        ),
        Ok(m) if m["mode"] == "server" => (
            false,
            format!(
                "not signed in to {0}, or the sign-in has ended: run `comindly login {0}`",
                c.base()
            ),
        ),
        Ok(_) => (false, format!("{} is not a comindly server", c.base())),
        Err(e) => (false, format!("{e:#}")),
    };
    comindly::doctor::Check {
        ok,
        optional: false,
        message,
    }
}

/// Run `cmd` against the server when it is one the server answers; `None` runs it locally.
pub fn dispatch(c: &Client, cmd: &Cmd) -> Option<ExitCode> {
    let q = |pairs: &[(&'static str, String)]| pairs.to_vec();
    let csv = |r: &[String]| r.join(",");
    Some(match cmd {
        Cmd::Find {
            query,
            index_dir: None,
            repo,
        } => show(
            c.get(
                "/api/v1/answer/find",
                &q(&[("q", query.clone()), ("repos", csv(repo))]),
            ),
            |hits: Vec<comindly::graph::Node>| crate::render_find(query, &hits),
        ),
        Cmd::Search {
            query,
            index_dir: None,
            repo,
            format,
            md,
        } => {
            let query = query.join(" ");
            let markdown = *md
                || format
                    .as_deref()
                    .is_none_or(|f| matches!(f, "md" | "markdown"));
            show(
                c.get(
                    "/api/v1/answer/search",
                    &q(&[("q", query.clone()), ("repos", csv(repo))]),
                ),
                |s| crate::render_search(&query, &s, markdown),
            )
        }
        Cmd::Why {
            focus,
            index_dir: None,
        } => show(
            c.get("/api/v1/answer/why", &q(&[("focus", focus.clone())])),
            |w| crate::render_why(&w),
        ),
        Cmd::Ledger {
            index_dir: None,
            inbox,
            check,
            compact: false,
        } => show(
            c.get(
                "/api/v1/answer/ledger",
                &q(&[("inbox", inbox.to_string()), ("check", check.to_string())]),
            ),
            |l| crate::render_ledger(&l),
        ),
        Cmd::Flow {
            focus,
            index_dir: None,
        } => show(
            c.get("/api/v1/answer/flow", &q(&[("focus", focus.clone())])),
            |f| crate::render_flow(&f),
        ),
        Cmd::Contracts {
            index_dir: None,
            kind,
            dangling,
        } => {
            let mut params = q(&[("dangling", dangling.to_string())]);
            params.extend(kind.clone().map(|k| ("kind", k)));
            show(c.get("/api/v1/answer/contracts", &params), |a| {
                crate::render_contracts(&a)
            })
        }
        Cmd::Conventions {
            what: None,
            index_dir: None,
            repo,
        } => show(
            c.get(
                "/api/v1/answer/conventions",
                &repo
                    .clone()
                    .map(|r| ("repo", r))
                    .into_iter()
                    .collect::<Vec<_>>(),
            ),
            |a| crate::render_conventions(&a),
        ),
        Cmd::Conventions {
            what: Some(what),
            index_dir: None,
            repo,
        } => {
            let body = match what {
                ConventionsWhat::Add { rule, dimension } => {
                    json!({"action": "add", "rule": rule, "dimension": dimension, "repo": repo})
                }
                ConventionsWhat::Edit { id, rule } => {
                    json!({"action": "edit", "id": id, "rule": rule})
                }
                ConventionsWhat::Rm { id } => json!({"action": "hide", "id": id}),
                ConventionsWhat::Restore { id } => json!({"action": "restore", "id": id}),
            };
            done(c.post("/api/v1/conventions", &body))
        }
        Cmd::Remember {
            kind,
            on,
            title,
            body,
            trigger,
            evidence,
            field,
            index_dir: None,
        } => {
            let fields: serde_json::Map<String, serde_json::Value> = field
                .iter()
                .filter_map(|f| f.split_once('='))
                .map(|(k, v)| (k.trim().to_string(), v.trim().into()))
                .collect();
            let req = json!({"kind": kind, "on": on, "title": title, "body": body,
                             "trigger": trigger, "evidence": evidence, "fields": fields});
            show(c.post("/api/v1/remember", &req), |r: serde_json::Value| {
                comindly::ui::ok(&format!(
                    "recorded {kind} `{title}` ({})",
                    r["id"].as_str().unwrap_or("")
                ));
                ExitCode::SUCCESS
            })
        }
        Cmd::Review {
            id,
            action,
            reason,
            index_dir: None,
        } => done(c.post(
            "/api/v1/review",
            &json!({"id": id, "action": action, "reason": reason}),
        )),
        Cmd::Link {
            repos,
            index_dir: None,
            no_llm,
            ..
        } => {
            if !repos.is_empty() {
                comindly::ui::err(
                    "a server indexes its own repository list; add repositories in its app (Settings → Repositories)",
                );
                return Some(ExitCode::FAILURE);
            }
            index(c, !*no_llm)
        }
        Cmd::Index {
            index_dir: None, ..
        } => {
            comindly::ui::err(
                "a server indexes its own repository list: run `comindly link` to start that, or --local for an index here",
            );
            ExitCode::FAILURE
        }
        _ => return None,
    })
}

/// Print an answer with `render`, or the server's error.
fn show<T, F>(answer: F, render: impl FnOnce(T) -> ExitCode) -> ExitCode
where
    F: std::future::Future<Output = anyhow::Result<T>> + Send,
    T: Send,
{
    match comindly::rt::block_on(answer) {
        Ok(v) => render(v),
        Err(e) => {
            comindly::ui::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

/// Print the server's message for a change.
fn done<F>(answer: F) -> ExitCode
where
    F: std::future::Future<Output = anyhow::Result<serde_json::Value>> + Send,
{
    show(answer, |r| {
        comindly::ui::ok(r["message"].as_str().unwrap_or("done"));
        ExitCode::SUCCESS
    })
}

/// Start or queue indexing on the server and follow it here; Ctrl-C cancels the job, not just
/// this view, and a queued one leaves the queue.
fn index(c: &Client, llm: bool) -> ExitCode {
    use comindly::events::EventSink;
    let run = async {
        let started: serde_json::Value = c
            .post(
                "/api/v1/index",
                &json!({"llm": llm, "drop_unlisted": false}),
            )
            .await?;
        let id = started["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("the server did not say which job started"))?
            .to_string();
        match started["position"].as_u64() {
            Some(_) => comindly::ui::note(&format!(
                "waiting on {} (job {id}); it starts when the jobs before it end",
                c.base()
            )),
            None => comindly::ui::note(&format!("indexing on {} (job {id})", c.base())),
        }
        let out = comindly::ui::Terminal::new();
        let follow = c.follow(&id, |e| out.emit(e));
        tokio::pin!(follow);
        tokio::select! {
            r = &mut follow => r?,
            _ = tokio::signal::ctrl_c() => {
                let r: serde_json::Value =
                    c.post(&format!("/api/v1/jobs/{id}/cancel"), &json!({})).await?;
                comindly::ui::warn(r["message"].as_str().unwrap_or("cancelling the job on the server"));
                follow.await?;
            }
        }
        let job: serde_json::Value = c.get(&format!("/api/v1/jobs/{id}"), &[]).await?;
        anyhow::Ok(job["job"]["status"].as_str().unwrap_or("").to_string())
    };
    match comindly::rt::block_on(run) {
        Ok(status) if status == "succeeded" => ExitCode::SUCCESS,
        Ok(status) => {
            comindly::ui::err(&format!("the job ended {status}"));
            ExitCode::FAILURE
        }
        Err(e) => {
            comindly::ui::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}
