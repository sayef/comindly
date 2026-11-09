//! Agent hooks: the context the current profile's engine has for a session or an edit, without
//! ever holding up the agent.

use std::collections::HashSet;
use std::process::ExitCode;
use std::time::Duration;

use comindly_api as api;

use crate::profile::{self, LOCAL};

const AGENTS: &[&str] = &["claude", "cursor", "gemini", "codex", "generic"];

/// The first string under `key` anywhere in a hook payload (agents nest tool input differently).
fn find_str<'v>(v: &'v serde_json::Value, key: &str) -> Option<&'v str> {
    match v {
        serde_json::Value::Object(m) => m
            .get(key)
            .and_then(|x| x.as_str())
            .or_else(|| m.values().find_map(|x| find_str(x, key))),
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_str(x, key)),
        _ => None,
    }
}

pub fn hook(flag: Option<&str>, agent: &str, event: &str) -> ExitCode {
    if !AGENTS.contains(&agent) {
        crate::term::err("agent must be claude, cursor, gemini, codex or generic");
        return ExitCode::FAILURE;
    }
    let mut input = String::new();
    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let cwd = find_str(&v, "cwd")
        .map(str::to_string)
        .or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|d| d.display().to_string())
        })
        .unwrap_or_default();
    let session = [
        "session_id",
        "sessionId",
        "conversation_id",
        "conversationId",
    ]
    .iter()
    .find_map(|k| find_str(&v, k))
    .unwrap_or("default")
    .replace(['/', '\\'], "_");
    let path = std::env::var("COMINDLY_HOOK_FILE").ok().or_else(|| {
        ["file_path", "filePath", "target_file", "path", "file"]
            .iter()
            .find_map(|k| find_str(&v, k))
            .map(str::to_string)
    });
    if event == "pre-edit" && path.is_none() {
        return ExitCode::SUCCESS;
    }
    // A hook never starts the local engine or waits long: no answer, no context.
    if profile::chosen(flag) == LOCAL && crate::engine::running().is_none() {
        return ExitCode::SUCCESS;
    }
    let Ok(c) = profile::client(flag) else {
        return ExitCode::SUCCESS;
    };
    let seen_file = std::env::temp_dir()
        .join("comindly-hook-sessions")
        .join(&session);
    let seen: HashSet<String> = std::fs::read_to_string(&seen_file)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let query = api::HookQuery {
        event: event.to_string(),
        cwd,
        path,
        seen: Some(seen.iter().cloned().collect::<Vec<_>>().join(",")).filter(|s| !s.is_empty()),
    };
    let ask = c.get::<api::answers::Hook>("/api/v1/answer/hook", &query);
    let Ok(Ok(h)) =
        crate::commands::block(async { tokio::time::timeout(Duration::from_secs(3), ask).await })
    else {
        return ExitCode::SUCCESS;
    };
    if h.text.is_empty() {
        return ExitCode::SUCCESS;
    }
    if let Some(dir) = seen_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut lines: Vec<String> = seen.into_iter().collect();
    lines.extend(h.ids);
    let _ = std::fs::write(&seen_file, lines.join("\n"));
    if agent != "claude" {
        println!("{}", h.text);
        return ExitCode::SUCCESS;
    }
    let name = if event == "session-start" {
        "SessionStart"
    } else {
        "PreToolUse"
    };
    let out = serde_json::json!({
        "hookSpecificOutput": { "hookEventName": name, "additionalContext": h.text }
    });
    println!("{out}");
    ExitCode::SUCCESS
}

pub fn init_hooks(agent: &str) -> ExitCode {
    if agent != "claude" && AGENTS.contains(&agent) {
        crate::term::note(&format!(
            "run these from {agent}'s hooks; each prints markdown to add to the agent's context:"
        ));
        println!("session start:   comindly hook {agent} session-start");
        println!("before an edit:  comindly hook {agent} pre-edit   (the file comes from the hook's JSON on stdin, or COMINDLY_HOOK_FILE)");
        return ExitCode::SUCCESS;
    }
    if agent != "claude" {
        crate::term::err("agent must be claude, cursor, gemini, codex or generic");
        return ExitCode::FAILURE;
    }
    let snippet = serde_json::json!({
        "hooks": {
            "SessionStart": [{
                "hooks": [{"type": "command", "command": "comindly hook claude session-start"}]
            }],
            "PreToolUse": [{
                "matcher": "Edit|MultiEdit|Write",
                "hooks": [{"type": "command", "command": "comindly hook claude pre-edit"}]
            }]
        }
    });
    crate::term::note("add to .claude/settings.json (project) or ~/.claude/settings.json (user):");
    println!(
        "{}",
        serde_json::to_string_pretty(&snippet).unwrap_or_default()
    );
    ExitCode::SUCCESS
}

pub fn init_agents(skill: bool) -> ExitCode {
    if skill {
        eprintln!("save as .claude/skills/comindly/SKILL.md in a repository, or in ~/.claude/skills/comindly/ for all:");
        print!("{}", api::agents::skill());
    } else {
        eprintln!("add to AGENTS.md or CLAUDE.md at the root of a repository:");
        print!("{}", api::agents::agents_md());
    }
    ExitCode::SUCCESS
}
