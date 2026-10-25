//! Commands answered by a comindly server: the same answers a local run computes, printed the same
//! way, and indexing as a job on the server that this terminal follows.

use std::process::ExitCode;

use comindly::api;
use comindly::client::Client;

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
    let me = comindly::rt::block_on(c.get::<api::Me>("/api/v1/auth/me", &()));
    let (ok, message) = match me {
        Ok(api::Me::Server {
            signed_in: true,
            email,
            role,
            ..
        }) => (
            true,
            format!(
                "signed in to {} as {} ({})",
                c.base(),
                email.as_deref().unwrap_or("?"),
                role.map_or("?".into(), |r| format!("{r:?}").to_lowercase())
            ),
        ),
        Ok(api::Me::Server { .. }) => (
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
    let csv = |r: &[String]| Some(r.join(",")).filter(|r| !r.is_empty());
    Some(match cmd {
        Cmd::Find {
            query,
            index_dir: None,
            repo,
        } => show(
            c.get(
                "/api/v1/answer/find",
                &api::AnswerQuery {
                    q: query.clone(),
                    repos: csv(repo),
                },
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
                    &api::AnswerQuery {
                        q: query.clone(),
                        repos: csv(repo),
                    },
                ),
                |s| crate::render_search(&query, &s, markdown),
            )
        }
        Cmd::Why {
            focus,
            index_dir: None,
        } => show(
            c.get(
                "/api/v1/answer/why",
                &api::AnswerFocus {
                    focus: focus.clone(),
                },
            ),
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
                &api::LedgerQuery {
                    inbox: *inbox,
                    check: *check,
                },
            ),
            |l| crate::render_ledger(&l),
        ),
        Cmd::Flow {
            focus,
            index_dir: None,
        } => show(
            c.get(
                "/api/v1/answer/flow",
                &api::AnswerFocus {
                    focus: focus.clone(),
                },
            ),
            |f| crate::render_flow(&f),
        ),
        Cmd::Explore {
            focus,
            index_dir: None,
            repos,
        } if repos.is_empty() => show(
            c.get(
                "/api/v1/answer/explore",
                &api::AnswerFocus {
                    focus: focus.clone(),
                },
            ),
            |e| crate::render_explore(focus, &e),
        ),
        Cmd::Stats {
            index_dir: None,
            repo,
        } => show(
            c.get(
                "/api/v1/answer/stats",
                &api::StatsQuery { repos: csv(repo) },
            ),
            |s| crate::render_stats(&s),
        ),
        Cmd::Packages {
            index_dir: None,
            skew,
        } => show(
            c.get(
                "/api/v1/answer/packages",
                &api::PackagesQuery { skew: *skew },
            ),
            |d: Vec<comindly::manifests::InternalDep>| crate::render_packages(&d),
        ),
        Cmd::Preflight {
            repo,
            base,
            strict,
            index_dir: None,
        } => {
            let changes = match crate::preflight_changes(repo, base) {
                Ok(c) => c,
                Err(code) => return Some(code),
            };
            show(
                c.post(
                    "/api/v1/answer/preflight",
                    &api::PreflightRequest { changes },
                ),
                |p| crate::render_preflight(&p, *strict),
            )
        }
        Cmd::Contracts {
            index_dir: None,
            kind,
            dangling,
        } => {
            let query = api::ContractsQuery {
                kind: kind.clone(),
                dangling: *dangling,
            };
            show(c.get("/api/v1/answer/contracts", &query), |a| {
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
                &api::RepoQuery { repo: repo.clone() },
            ),
            |a| crate::render_conventions(&a),
        ),
        Cmd::Conventions {
            what: Some(what),
            index_dir: None,
            repo,
        } => {
            let change = |action: &str, id: &str, rule: &str| api::ConventionChange {
                action: action.into(),
                id: id.into(),
                repo: None,
                dimension: None,
                rule: rule.into(),
            };
            let body = match what {
                ConventionsWhat::Add { rule, dimension } => api::ConventionChange {
                    repo: repo.clone(),
                    dimension: Some(dimension.clone()),
                    ..change("add", "", rule)
                },
                ConventionsWhat::Edit { id, rule } => change("edit", id, rule),
                ConventionsWhat::Rm { id } => change("hide", id, ""),
                ConventionsWhat::Restore { id } => change("restore", id, ""),
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
            let req = api::RememberRequest {
                kind: kind.clone(),
                on: on.clone(),
                title: title.clone(),
                body: body.clone(),
                trigger: trigger.clone(),
                evidence: evidence.clone(),
                fields: field
                    .iter()
                    .filter_map(|f| f.split_once('='))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect(),
            };
            show(c.post("/api/v1/remember", &req), |r: api::Remembered| {
                comindly::ui::ok(&format!("recorded {kind} `{title}` ({})", r.id));
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
            &api::ReviewRequest {
                id: id.clone(),
                action: action.clone(),
                reason: reason.clone(),
            },
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
        Cmd::Ui {
            index_dir: None,
            web: false,
            ..
        } => match comindly::tui::fetch(c)
            .and_then(|data| comindly::tui::App::new(data).on_server(c.clone()).run())
        {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                comindly::ui::err(&format!("{e:#}"));
                ExitCode::FAILURE
            }
        },
        #[cfg(target_os = "macos")]
        Cmd::App { index_dir: None } => {
            match comindly::app::run(comindly::app::Open::Server(c.base().to_string())) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    comindly::ui::err(&format!("{e:#}"));
                    ExitCode::FAILURE
                }
            }
        }
        #[cfg(not(target_os = "macos"))]
        Cmd::App { index_dir: None } => {
            crate::open_in_browser(c.base());
            comindly::ui::ok(&format!(
                "opened {} in your browser; sign in there",
                c.base()
            ));
            ExitCode::SUCCESS
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
    F: std::future::Future<Output = anyhow::Result<api::Done>> + Send,
{
    show(answer, |r| {
        comindly::ui::ok(&r.message);
        ExitCode::SUCCESS
    })
}

/// Start or queue indexing on the server and follow it here; Ctrl-C cancels the job, not just
/// this view, and a queued one leaves the queue.
fn index(c: &Client, llm: bool) -> ExitCode {
    use comindly::events::EventSink;
    let run = async {
        let started: api::IndexStarted = c
            .post(
                "/api/v1/index",
                &api::IndexRequest {
                    llm,
                    drop_unlisted: false,
                },
            )
            .await?;
        let id = started.id;
        match started.position {
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
                let r: api::Done = c
                    .post(&format!("/api/v1/jobs/{id}/cancel"), &serde_json::Map::new())
                    .await?;
                comindly::ui::warn(&r.message);
                follow.await?;
            }
        }
        let job: api::JobDetail = c.get(&format!("/api/v1/jobs/{id}"), &()).await?;
        anyhow::Ok(job.job.status)
    };
    match comindly::rt::block_on(run) {
        Ok(api::Status::Succeeded) => ExitCode::SUCCESS,
        Ok(status) => {
            comindly::ui::err(&format!(
                "the job ended {}",
                format!("{status:?}").to_lowercase()
            ));
            ExitCode::FAILURE
        }
        Err(e) => {
            comindly::ui::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}
