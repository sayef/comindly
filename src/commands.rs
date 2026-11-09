//! Each command as calls to an engine's `/api/v1`, printed with [`crate::render`].

use std::path::Path;
use std::process::ExitCode;

use anyhow::{bail, Context, Result};
use comindly_api::client::Client;
use comindly_api::{self as api, prompts::Task};

use crate::profile::{self, Profile, Profiles, LOCAL};
use crate::{render, term};
use crate::{Cmd, ConfigWhat, ConventionsWhat, EngineWhat, PromptsWhat, ReposWhat, TeamWhat};

/// Run a future on this command's runtime.
pub fn block<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(f)
}

fn fail(e: anyhow::Error) -> ExitCode {
    term::err(&format!("{e:#}"));
    ExitCode::FAILURE
}

/// Print an answer with `render`, or the engine's error.
fn show<T>(
    answer: impl std::future::Future<Output = Result<T>>,
    render: impl FnOnce(T) -> ExitCode,
) -> ExitCode {
    match block(answer) {
        Ok(v) => render(v),
        Err(e) => fail(e),
    }
}

/// Print the engine's message for a change.
fn done(answer: impl std::future::Future<Output = Result<api::Done>>) -> ExitCode {
    show(answer, |r| {
        term::ok(&r.message);
        ExitCode::SUCCESS
    })
}

pub fn run(flag: Option<&str>, cmd: Cmd) -> ExitCode {
    match cmd {
        Cmd::Use { target, name } => to_code(use_profile(target, name)),
        Cmd::Engine { what } => to_code(engine(what)),
        Cmd::Login => to_code(login(flag)),
        Cmd::Logout => to_code(logout(flag)),
        Cmd::Doctor => crate::doctor::run(flag),
        Cmd::Hook { agent, event } => crate::hooks::hook(flag, &agent, &event),
        Cmd::InitHooks { agent } => crate::hooks::init_hooks(&agent),
        Cmd::InitAgents { skill } => crate::hooks::init_agents(skill),
        Cmd::Serve => crate::mcp::serve(flag),
        Cmd::Changed { repo, since } => to_code(changed(&repo, since.as_deref())),
        cmd => match profile::client(flag) {
            Ok(c) => with_engine(flag, &c, cmd),
            Err(e) => fail(e),
        },
    }
}

fn to_code(r: Result<()>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e),
    }
}

fn with_engine(flag: Option<&str>, c: &Client, cmd: Cmd) -> ExitCode {
    let csv = |r: &[String]| Some(r.join(",")).filter(|r| !r.is_empty());
    match cmd {
        Cmd::Find { query, repo, team } => show(
            c.get(
                "/api/v1/answer/find",
                &api::AnswerQuery {
                    q: query.clone(),
                    repos: csv(&repo),
                    teams: csv(&team),
                },
            ),
            |hits: Vec<api::graph::Node>| render::render_find(&query, &hits),
        ),
        Cmd::Search {
            query,
            repo,
            team,
            format,
        } => {
            let query = query.join(" ");
            let markdown = format
                .as_deref()
                .is_none_or(|f| matches!(f, "md" | "markdown"));
            show(
                c.get(
                    "/api/v1/answer/search",
                    &api::AnswerQuery {
                        q: query.clone(),
                        repos: csv(&repo),
                        teams: csv(&team),
                    },
                ),
                |s| render::render_search(&query, &s, markdown),
            )
        }
        Cmd::Why { focus } => show(
            c.get("/api/v1/answer/why", &api::AnswerFocus { focus }),
            |w| render::render_why(&w),
        ),
        Cmd::Ledger { inbox, check } => show(
            c.get("/api/v1/answer/ledger", &api::LedgerQuery { inbox, check }),
            |l| render::render_ledger(&l),
        ),
        Cmd::Flow { focus } => show(
            c.get("/api/v1/answer/flow", &api::AnswerFocus { focus }),
            |f| render::render_flow(&f),
        ),
        Cmd::Explore { focus } => show(
            c.get(
                "/api/v1/answer/explore",
                &api::AnswerFocus {
                    focus: focus.clone(),
                },
            ),
            |e| render::render_explore(&focus, &e),
        ),
        Cmd::Stats { repo } => show(
            c.get(
                "/api/v1/answer/stats",
                &api::StatsQuery { repos: csv(&repo) },
            ),
            |s| render::render_stats(&s),
        ),
        Cmd::Guide { repo, output } => show(
            c.get::<Vec<(String, String)>>(
                "/api/v1/answer/guide",
                &api::StatsQuery { repos: csv(&repo) },
            ),
            |guides| guide(&guides, output.as_deref()),
        ),
        Cmd::Packages { skew } => show(
            c.get("/api/v1/answer/packages", &api::PackagesQuery { skew }),
            |d: Vec<api::manifests::InternalDep>| render::render_packages(&d),
        ),
        Cmd::Contracts { kind, dangling } => show(
            c.get(
                "/api/v1/answer/contracts",
                &api::ContractsQuery { kind, dangling },
            ),
            |a| render::render_contracts(&a),
        ),
        Cmd::Preflight { repo, base, strict } => {
            let changes = match preflight_changes(&repo, &base) {
                Ok(Some(c)) => c,
                Ok(None) => {
                    term::ok("no changes against the base");
                    return ExitCode::SUCCESS;
                }
                Err(e) => return fail(e),
            };
            show(
                c.post(
                    "/api/v1/answer/preflight",
                    &api::PreflightRequest { changes },
                ),
                |p| render::render_preflight(&p, strict),
            )
        }
        Cmd::Conventions { what: None, repo } => show(
            c.get("/api/v1/answer/conventions", &api::RepoQuery { repo }),
            |a| render::render_conventions(&a),
        ),
        Cmd::Conventions {
            what: Some(what),
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
                    repo,
                    dimension: Some(dimension),
                    ..change("add", "", &rule)
                },
                ConventionsWhat::Edit { id, rule } => change("edit", &id, &rule),
                ConventionsWhat::Rm { id } => change("hide", &id, ""),
                ConventionsWhat::Restore { id } => change("restore", &id, ""),
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
            snippet,
        } => {
            let snippets = match read_snippets(&snippet) {
                Ok(s) => s,
                Err(e) => return fail(e),
            };
            let req = api::RememberRequest {
                kind: kind.clone(),
                on,
                title: title.clone(),
                body,
                trigger,
                evidence,
                fields: field
                    .iter()
                    .filter_map(|f| f.split_once('='))
                    .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
                    .collect(),
                snippets,
            };
            show(c.post("/api/v1/remember", &req), |r: api::Remembered| {
                term::ok(&format!("recorded {kind} `{title}` ({})", r.id));
                ExitCode::SUCCESS
            })
        }
        Cmd::Review { id, action, reason } => {
            done(c.post("/api/v1/review", &api::ReviewRequest { id, action, reason }))
        }
        Cmd::Erase { ids, yes } => {
            if !yes {
                term::note(&format!(
                    "would erase {}; add --yes to erase them",
                    ids.join(", ")
                ));
                return ExitCode::SUCCESS;
            }
            let mut code = ExitCode::SUCCESS;
            for id in ids {
                if done(c.post("/api/v1/erase", &api::IdRequest { id })) != ExitCode::SUCCESS {
                    code = ExitCode::FAILURE;
                }
            }
            code
        }
        Cmd::Link {
            folders,
            no_llm,
            rewrite,
        } => {
            let mut tasks = Vec::new();
            for t in &rewrite {
                match Task::parse(t.trim()) {
                    Some(t) => tasks.push(t),
                    None => {
                        return fail(anyhow::anyhow!(
                            "`{t}` is not a task: summary, guide or flow"
                        ))
                    }
                }
            }
            match folders_here(flag, &folders) {
                Ok(paths) => index(c, !no_llm, tasks, paths),
                Err(e) => fail(e),
            }
        }
        Cmd::Index { folder, no_llm } => match folders_here(flag, &[folder]) {
            Ok(paths) => index(c, !no_llm, Vec::new(), paths),
            Err(e) => fail(e),
        },
        Cmd::Repos { what } => to_code(repos(c, what)),
        Cmd::Team { what } => to_code(team(c, what)),
        Cmd::Prompts { what } => to_code(prompts(c, what)),
        Cmd::Config { what } => to_code(config(c, what)),
        Cmd::App => to_code(app(flag, c)),
        _ => unreachable!("handled before an engine is needed"),
    }
}

/// Absolute folders for the local engine; other engines index their own list.
fn folders_here(flag: Option<&str>, folders: &[String]) -> Result<Vec<String>> {
    if folders.is_empty() {
        return Ok(Vec::new());
    }
    if profile::chosen(flag) != LOCAL {
        bail!("a server indexes its own repository list; add repositories in its app (Settings → Repositories), or `comindly use local` to index folders here");
    }
    folders
        .iter()
        .map(|f| {
            std::fs::canonicalize(f)
                .map(|p| p.to_string_lossy().into_owned())
                .with_context(|| format!("no folder {f}"))
        })
        .collect()
}

/// Start or queue indexing and follow it here; Ctrl-C cancels the job, not just this view.
fn index(c: &Client, llm: bool, rewrite: Vec<Task>, paths: Vec<String>) -> ExitCode {
    let run = async {
        let started: api::IndexStarted = c
            .post(
                "/api/v1/index",
                &api::IndexRequest {
                    llm,
                    drop_unlisted: false,
                    rewrite,
                    paths,
                },
            )
            .await?;
        let id = started.id;
        match started.position {
            Some(_) => term::note(&format!(
                "waiting on {} (job {id}); it starts when the jobs before it end",
                c.base()
            )),
            None => term::note(&format!("indexing on {} (job {id})", c.base())),
        }
        let mut out = term::Events::default();
        let follow = c.follow(&id, |e| out.show(e));
        tokio::pin!(follow);
        tokio::select! {
            r = &mut follow => r?,
            _ = tokio::signal::ctrl_c() => {
                let r: api::Done = c
                    .post(&format!("/api/v1/jobs/{id}/cancel"), &serde_json::Map::new())
                    .await?;
                term::warn(&r.message);
                follow.await?;
            }
        }
        let job: api::JobDetail = c.get(&format!("/api/v1/jobs/{id}"), &()).await?;
        anyhow::Ok(job.job.status)
    };
    match block(run) {
        Ok(api::Status::Succeeded) => ExitCode::SUCCESS,
        Ok(status) => fail(anyhow::anyhow!(
            "the job ended {}",
            format!("{status:?}").to_lowercase()
        )),
        Err(e) => fail(e),
    }
}

fn guide(guides: &[(String, String)], output: Option<&str>) -> ExitCode {
    if guides.is_empty() {
        term::note("no style guide yet: index with the LLM steps on");
        return ExitCode::SUCCESS;
    }
    let doc: String = guides
        .iter()
        .map(|(repo, g)| format!("# Style guide — {repo}\n\n{g}\n"))
        .collect::<Vec<_>>()
        .join("\n");
    match output {
        Some(path) => match std::fs::write(path, doc) {
            Ok(()) => {
                term::ok(&format!("wrote {path}"));
                ExitCode::SUCCESS
            }
            Err(e) => fail(e.into()),
        },
        None => {
            println!("{doc}");
            ExitCode::SUCCESS
        }
    }
}

/// `path:START-END` snippets from files here, with the commit they were read at.
fn read_snippets(specs: &[String]) -> Result<Vec<api::ledger::Snippet>> {
    specs
        .iter()
        .map(|spec| {
            let (path, range) = spec
                .rsplit_once(':')
                .with_context(|| format!("snippet `{spec}`: use path:START-END"))?;
            let (a, b) = range.split_once('-').unwrap_or((range, range));
            let (a, b): (usize, usize) = (a.trim().parse()?, b.trim().parse()?);
            anyhow::ensure!(
                a >= 1 && b >= a,
                "snippet `{spec}`: START-END counts from 1"
            );
            let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
            let code: Vec<&str> = text.lines().skip(a - 1).take(b - a + 1).collect();
            let dir = Path::new(path)
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Ok(api::ledger::Snippet {
                path: path.to_string(),
                code: code.join("\n"),
                commit: crate::git::head_commit(dir).ok(),
            })
        })
        .collect()
}

/// This repository's changes against `base`, named as the index names the repository.
fn preflight_changes(repo: &str, base: &str) -> Result<Option<Vec<api::preflight::Change>>> {
    let root = Path::new(repo);
    let name = std::fs::canonicalize(root)?
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.to_string());
    let raw = crate::git::working_changes(root, base)?;
    if raw.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        raw.into_iter()
            .map(|(path, lines)| api::preflight::Change {
                repo: name.clone(),
                path,
                lines,
            })
            .collect(),
    ))
}

fn changed(repo: &str, since: Option<&str>) -> Result<()> {
    let dir = Path::new(repo);
    term::field("HEAD", &crate::git::head_commit(dir)?);
    let Some(base) = since else {
        term::note("pass --since <commit> to list changed files");
        return Ok(());
    };
    let [added, modified, deleted] = crate::git::changed_files(dir, base)?;
    term::header(&format!(
        "changed since {base}: {} files ({} added, {} modified, {} deleted)",
        added.len() + modified.len() + deleted.len(),
        added.len(),
        modified.len(),
        deleted.len()
    ));
    for (mark, files) in [("+", &added), ("~", &modified), ("-", &deleted)] {
        for p in files.iter().take(10) {
            println!("  {mark} {p}");
        }
        if files.len() > 10 {
            term::note(&format!("… and {} more", files.len() - 10));
        }
    }
    Ok(())
}

fn use_profile(target: Option<String>, name: Option<String>) -> Result<()> {
    let mut all = Profiles::load();
    let Some(target) = target else {
        let active = all.active.clone().unwrap_or_else(|| LOCAL.into());
        let mark = |n: &str| if n == active { "*" } else { " " };
        let local = match crate::engine::running() {
            Some(r) => format!("this machine, running at {}", r.url),
            None => "this machine, started when needed".into(),
        };
        println!("{} {LOCAL:<12} {local}", mark(LOCAL));
        for (n, p) in &all.profiles {
            let who = p
                .email
                .as_deref()
                .map(|e| format!("signed in as {e}"))
                .unwrap_or_else(|| "not signed in".into());
            println!("{} {n:<12} {}  ({who})", mark(n), p.url);
        }
        return Ok(());
    };
    if target.starts_with("http://") || target.starts_with("https://") {
        let name = name.context("name it: `comindly use <url> <name>`")?;
        anyhow::ensure!(
            name != LOCAL,
            "`local` is the engine on this machine; pick another name"
        );
        let url = target.trim_end_matches('/').to_string();
        let entry = all.profiles.entry(name.clone()).or_default();
        if entry.url != url {
            *entry = Profile {
                url,
                ..Profile::default()
            };
        }
        all.active = Some(name.clone());
        all.save()?;
        term::ok(&format!("using {name}"));
        if all.profiles[&name].token.is_none() {
            return login(Some(&name));
        }
        return Ok(());
    }
    if target != LOCAL && !all.profiles.contains_key(&target) {
        bail!("no profile named `{target}`: add it with `comindly use <url> {target}`");
    }
    all.active = Some(target.clone());
    all.save()?;
    if target == LOCAL {
        let url = crate::engine::ensure_running()?;
        term::ok(&format!("using the engine on this machine ({url})"));
    } else {
        term::ok(&format!("using {target} ({})", all.profiles[&target].url));
    }
    Ok(())
}

fn engine(what: EngineWhat) -> Result<()> {
    match what {
        EngineWhat::Start => {
            let url = crate::engine::ensure_running()?;
            term::ok(&format!("the local engine runs at {url}"));
        }
        EngineWhat::Stop => match crate::engine::stop()? {
            true => term::ok("stopped the local engine"),
            false => term::note("the local engine was not running"),
        },
        EngineWhat::Status => {
            match crate::engine::binary() {
                Some(b) => term::field("installed", &b.display().to_string()),
                None => term::field("installed", "no: `comindly engine install`"),
            }
            match crate::engine::running() {
                Some(r) => term::field("running", &format!("{} (version {})", r.url, r.version)),
                None => term::field("running", "no"),
            }
        }
        EngineWhat::Install => {
            let at = block(crate::engine::install())?;
            term::ok(&format!("installed the engine at {}", at.display()));
        }
    }
    Ok(())
}

fn login(flag: Option<&str>) -> Result<()> {
    let name = profile::chosen(flag);
    anyhow::ensure!(
        name != LOCAL,
        "the engine on this machine needs no sign-in; `comindly use <url> <name>` adds a server"
    );
    let url = Profiles::load()
        .profiles
        .get(&name)
        .map(|p| p.url.clone())
        .context("no such profile")?;
    let host = std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "this machine".into());
    let client = format!("comindly CLI on {host}");
    let a = block(comindly_api::client::login(&url, &client, |link, code| {
        term::ok(&format!("approve this sign-in in your browser: {link}"));
        term::field("code", code);
        term::note(
            "check that the page shows the same code; waiting for approval (Ctrl-C to stop)",
        );
        if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
            open_in_browser(link);
        }
    }))?;
    profile::set_sign_in(&name, Some(a.token), Some(a.email.clone()))?;
    term::ok(&format!("signed in to {url} as {}", a.email));
    if let Some(day) = a.expires.as_deref().and_then(|e| e.get(..10)) {
        term::note(&format!(
            "this sign-in lasts until {day}; then run `comindly login` again"
        ));
    }
    Ok(())
}

fn logout(flag: Option<&str>) -> Result<()> {
    let name = profile::chosen(flag);
    let all = Profiles::load();
    let Some(p) = all.profiles.get(&name).filter(|p| p.token.is_some()) else {
        term::note("not signed in");
        return Ok(());
    };
    let c = Client::new(&p.url, p.token.clone());
    profile::set_sign_in(&name, None, None)?;
    term::ok(&format!("signed out of {} on this machine", p.url));
    match block(c.revoke_own_token()) {
        Ok(true) => term::ok("revoked its access token on the server"),
        Ok(false) => term::note("the server no longer accepted its access token"),
        Err(e) => {
            term::warn(&format!(
                "could not revoke the access token on the server: {e:#}"
            ));
            term::note("it stays valid until you revoke it in the app (Settings → Your access)");
        }
    }
    Ok(())
}

fn repos(c: &Client, what: Option<ReposWhat>) -> Result<()> {
    match what {
        None => {
            let r: api::Repos = block(c.get("/api/v1/repos", &()))?;
            let mut t = term::table(&["repo", "folder", "branch", "symbols"]);
            for row in &r.repos {
                t.add_row(vec![
                    row.name.clone(),
                    row.path.clone(),
                    row.branch.clone().unwrap_or_default(),
                    row.symbols.map(|n| n.to_string()).unwrap_or_default(),
                ]);
            }
            term::right_align(&mut t, &[3]);
            println!("{t}");
            for u in &r.unlisted {
                term::note(&format!(
                    "{} is indexed but has no folder in the list",
                    u.name
                ));
            }
        }
        Some(ReposWhat::Add { paths }) => {
            let add = paths
                .iter()
                .map(|p| {
                    std::fs::canonicalize(p)
                        .map(|c| c.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| p.clone())
                })
                .collect();
            let r: api::Done = block(c.post(
                "/api/v1/repos",
                &api::RepoChange {
                    add,
                    ..Default::default()
                },
            ))?;
            term::ok(&r.message);
        }
        Some(ReposWhat::Remove { repos }) => {
            let r: api::Done = block(c.post(
                "/api/v1/repos",
                &api::RepoChange {
                    remove: repos,
                    ..Default::default()
                },
            ))?;
            term::ok(&r.message);
        }
        Some(ReposWhat::Scan { dir, depth }) => {
            let dir = std::fs::canonicalize(&dir)
                .map(|c| c.to_string_lossy().into_owned())
                .unwrap_or(dir);
            let s: api::Scan = block(c.post(
                "/api/v1/repos/scan",
                &api::ScanRequest {
                    dir,
                    depth: Some(depth),
                },
            ))?;
            for hit in &s.found {
                let mark = if hit.listed { "listed" } else { "" };
                println!("  {:<28} {} {mark}", hit.name, hit.path);
            }
            term::note("add them with `comindly repos add <folders>`");
        }
    }
    Ok(())
}

fn team(c: &Client, what: Option<TeamWhat>) -> Result<()> {
    let t: api::Team = block(c.get("/api/v1/team", &()))?;
    match what {
        None => {
            term::header("Team settings");
            if let serde_json::Value::Object(m) = serde_json::to_value(&t.settings)? {
                for (k, v) in m {
                    let shown = match v {
                        serde_json::Value::Null => "(comindly default)".to_string(),
                        serde_json::Value::String(s) => s,
                        v => v.to_string(),
                    };
                    term::field(&k, &shown);
                }
            }
            term::field(
                "your API key",
                &format!("{:?}", t.key_source).to_lowercase(),
            );
        }
        Some(TeamWhat::Set { pairs }) => {
            let mut v = serde_json::to_value(&t.settings)?;
            let m = v.as_object_mut().context("team settings")?;
            for pair in &pairs {
                let (k, val) = pair
                    .split_once('=')
                    .with_context(|| format!("`{pair}`: use KEY=VALUE"))?;
                let val = val.trim();
                let value = if val.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::from_str(val).unwrap_or_else(|_| val.into())
                };
                m.insert(k.trim().to_string(), value);
            }
            let settings = serde_json::from_value(v).context("team settings")?;
            let r: api::Done = block(c.post(
                "/api/v1/team",
                &api::TeamSave {
                    settings,
                    version: t.version,
                },
            ))?;
            term::ok(&r.message);
        }
        Some(TeamWhat::Key { stdin }) => {
            let key = if stdin {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)?;
                s.trim().to_string()
            } else {
                rpassword::prompt_password("LLM API key (empty removes it): ")?
            };
            let r: api::Done = block(c.post("/api/v1/team/key", &api::KeyRequest { key }))?;
            term::ok(&r.message);
        }
    }
    Ok(())
}

fn task(name: &str) -> Result<Task> {
    Task::parse(name).with_context(|| format!("`{name}` is not a task: summary, guide or flow"))
}

fn prompts(c: &Client, what: Option<PromptsWhat>) -> Result<()> {
    let p: api::Prompts = block(c.get("/api/v1/prompts", &()))?;
    let change = |action: api::PromptAction| -> Result<()> {
        let r: api::Done = block(c.post(
            "/api/v1/prompts",
            &api::PromptChange {
                version: p.version,
                action,
            },
        ))?;
        term::ok(&r.message);
        Ok(())
    };
    match what {
        None => {
            for t in &p.tasks {
                let active = t
                    .active
                    .map(|n| format!("version {n}"))
                    .unwrap_or_else(|| "built-in".into());
                term::field(
                    t.task.name(),
                    &format!("{active}; {} saved", t.versions.len()),
                );
            }
            Ok(())
        }
        Some(PromptsWhat::Show {
            task: name,
            version,
        }) => {
            let want = task(&name)?;
            let t = p
                .tasks
                .iter()
                .find(|t| t.task == want)
                .context("no such task")?;
            let text = match version.or(t.active) {
                Some(n) => t
                    .versions
                    .iter()
                    .find(|v| v.version == n)
                    .map(|v| v.text.clone())
                    .with_context(|| format!("no version {n}"))?,
                None => t.builtin.clone(),
            };
            println!("{text}\n\n# Always added\n{}", t.format);
            Ok(())
        }
        Some(PromptsWhat::Save {
            task: name,
            file,
            note,
        }) => change(api::PromptAction::Save {
            task: task(&name)?,
            text: std::fs::read_to_string(&file).with_context(|| format!("read {file}"))?,
            sections: Default::default(),
            note,
        }),
        Some(PromptsWhat::Activate {
            task: name,
            version,
        }) => change(api::PromptAction::Activate {
            task: task(&name)?,
            n: version,
        }),
        Some(PromptsWhat::Deactivate { task: name }) => {
            change(api::PromptAction::Deactivate { task: task(&name)? })
        }
        Some(PromptsWhat::Delete {
            task: name,
            version,
        }) => change(api::PromptAction::Delete {
            task: task(&name)?,
            n: version,
        }),
    }
}

fn config(c: &Client, what: Option<ConfigWhat>) -> Result<()> {
    match what.unwrap_or(ConfigWhat::Show) {
        ConfigWhat::Show => {
            let s: api::Settings = block(c.get("/api/v1/settings", &()))?;
            term::field("index_dir", &s.index_effective);
            term::field(
                "author_name",
                s.author_effective.as_deref().unwrap_or("(from git)"),
            );
            term::field("record_names", &s.record_names.to_string());
            term::field("cache_ttl_secs", &s.cache_ttl_secs.to_string());
            term::field("llm_model", &s.llm_model);
            term::note(&format!("stored in {}", s.config_path));
        }
        ConfigWhat::Set { pairs } => {
            let mut change = api::SettingsChange::default();
            for pair in &pairs {
                let (k, v) = pair
                    .split_once('=')
                    .with_context(|| format!("`{pair}`: use KEY=VALUE"))?;
                let v = v.trim().to_string();
                match k.trim() {
                    "index_dir" => change.index_dir = Some(v),
                    "author_name" => change.author_name = Some(v),
                    "record_names" => change.record_names = Some(v.parse()?),
                    "cache_ttl_secs" => change.cache_ttl_secs = Some(v.parse()?),
                    k => bail!("unknown setting `{k}`: index_dir, author_name, record_names or cache_ttl_secs"),
                }
            }
            let r: api::Done = block(c.post("/api/v1/settings", &change))?;
            term::ok(&r.message);
        }
    }
    Ok(())
}

/// The local engine's token, which signs its app in; a server's app asks for its own sign-in.
fn local_token(flag: Option<&str>) -> Option<String> {
    (profile::chosen(flag) == LOCAL)
        .then(crate::engine::local_token)
        .flatten()
}

fn app(flag: Option<&str>, c: &Client) -> Result<()> {
    let token = local_token(flag);
    if cfg!(target_os = "macos") {
        if let Some(bin) = crate::engine::binary() {
            let mut cmd = std::process::Command::new(bin);
            cmd.args(["window", c.base()]);
            if let Some(t) = &token {
                cmd.env("COMINDLY_TOKEN", t);
            }
            anyhow::ensure!(cmd.status()?.success(), "the app window failed");
            return Ok(());
        }
    }
    let url = match token {
        Some(t) => format!("{}/?token={t}", c.base()),
        None => format!("{}/", c.base()),
    };
    open_in_browser(&url);
    term::ok(&format!("opened {} in your browser", c.base()));
    Ok(())
}

pub fn open_in_browser(url: &str) {
    let mut cmd = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = cmd.arg(url).spawn();
}
