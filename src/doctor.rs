//! `comindly doctor`: the current profile, its engine, and how agents on this machine use it.

use std::process::ExitCode;

use comindly_api as api;

use crate::profile::{self, LOCAL};
use crate::term;

struct Check {
    ok: bool,
    message: String,
}

fn check(ok: bool, good: impl Into<String>, bad: impl Into<String>) -> Check {
    Check {
        ok,
        message: if ok { good.into() } else { bad.into() },
    }
}

pub fn run(flag: Option<&str>) -> ExitCode {
    term::header("comindly doctor");
    let name = profile::chosen(flag);
    let mut checks = Vec::new();
    if name == LOCAL {
        term::field("profile", "local (the engine on this machine)");
        let bin = crate::engine::binary();
        checks.push(check(
            bin.is_some(),
            "local engine installed",
            "the local engine is not installed: `comindly engine install`",
        ));
        if let Some(r) = crate::engine::running() {
            checks.push(check(
                r.version == env!("CARGO_PKG_VERSION"),
                format!("local engine running at {}", r.url),
                format!(
                    "the local engine is {} and this command {}: `comindly engine stop`, then run a command to start the matching one",
                    r.version,
                    env!("CARGO_PKG_VERSION")
                ),
            ));
        } else if bin.is_some() {
            term::note("the local engine starts when a command needs it");
        }
    }
    let client = match (name.as_str(), crate::engine::running()) {
        (LOCAL, None) => None,
        _ => profile::client(flag).ok(),
    };
    if let Some(c) = &client {
        if name != LOCAL {
            term::field("profile", &format!("{name} ({})", c.base()));
        }
        checks.extend(engine_checks(c));
    }
    let agents = api::agents::agents();
    checks.push(check(
        !agents.is_empty(),
        format!("comindly's tools are set up for {}", agents.join(", ")),
        "no agent uses comindly's tools yet: the app's Settings → Your access shows how to add them",
    ));
    let hooks = ["~/.claude/settings.json", ".claude/settings.json"]
        .iter()
        .map(|p| p.replacen('~', &std::env::var("HOME").unwrap_or_default(), 1))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .any(|s| s.contains("comindly hook"));
    checks.push(check(
        hooks,
        "Claude Code hooks configured",
        "Claude Code hooks not found (optional): `comindly init-hooks claude`",
    ));
    for c in &checks {
        if c.ok {
            term::ok(&c.message);
        } else {
            term::warn(&c.message);
        }
    }
    if checks.iter().all(|c| c.ok) {
        term::ok("all good");
    }
    ExitCode::SUCCESS
}

fn engine_checks(c: &comindly_api::client::Client) -> Vec<Check> {
    let me = crate::commands::block(c.get::<api::Me>("/api/v1/auth/me", &()));
    let sign_in = match me {
        Ok(api::Me::Local) => check(true, "the engine answers", ""),
        Ok(api::Me::Server {
            signed_in: true,
            email,
            role,
            ..
        }) => check(
            true,
            format!(
                "signed in as {} ({})",
                email.as_deref().unwrap_or("?"),
                role.map_or("?", |r| r.as_str())
            ),
            "",
        ),
        Ok(api::Me::Server { .. }) => check(
            false,
            "",
            "not signed in, or the sign-in has ended: `comindly login`",
        ),
        Err(e) => check(false, "", format!("{e:#}")),
    };
    if !sign_in.ok {
        return vec![sign_in];
    }
    let stats = crate::commands::block(
        c.get::<api::answers::Stats>("/api/v1/answer/stats", &api::StatsQuery { repos: None }),
    );
    let index = match stats {
        Ok(s) => check(
            s.indexed > 0,
            format!(
                "index: {} across {}",
                crate::render::plural(s.symbols, "symbol"),
                crate::render::plural(s.indexed, "repo")
            ),
            "nothing indexed yet: `comindly link <folders>` here, or add repositories in the app",
        ),
        Err(e) => check(false, "", format!("the index is not readable: {e:#}")),
    };
    vec![sign_in, index]
}
