//! `comindly init-agents --plugin|--install`: one plugin per agent with comindly's MCP server, its
//! skill and its hooks, written from the same text `init-agents` and `init-hooks` print. A plugin
//! never names a server: `comindly serve` and the hooks follow the profile in use.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use comindly_api as api;
use serde_json::json;

/// Agents a plugin is written for.
pub const AGENTS: &[&str] = &["claude", "gemini", "codex", "cursor"];

const DESCRIPTION: &str =
    "comindly's tools for coding agents: one graph of the team's repositories, \
their service contracts, conventions and recorded decisions.";

/// The files of `agent`'s plugin, by path inside it. `exe` is how the plugin starts comindly; with
/// `gateway`, the agent connects to that address and signs in there instead of running `serve`.
pub fn files(agent: &str, exe: &str, gateway: Option<&str>) -> Result<Vec<(PathBuf, String)>> {
    let version = env!("CARGO_PKG_VERSION");
    let hook = |event: &str| format!("{exe} hook {agent} {event}");
    let serve = match (agent, gateway) {
        (_, None) => json!({ "command": exe, "args": ["serve"] }),
        ("gemini", Some(url)) => json!({ "httpUrl": url }),
        ("cursor", Some(url)) => json!({ "url": url }),
        (_, Some(url)) => json!({ "type": "http", "url": url }),
    };
    let pretty = |v: serde_json::Value| serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    let skill = (
        PathBuf::from("skills/comindly/SKILL.md"),
        api::agents::skill(),
    );
    let mut out = vec![skill];
    match agent {
        "claude" => {
            out.push((
                ".claude-plugin/plugin.json".into(),
                pretty(
                    json!({ "name": "comindly", "version": version, "description": DESCRIPTION,
                    "author": { "name": "comindly" }, "license": "MIT" }),
                ),
            ));
            out.push((
                ".mcp.json".into(),
                pretty(json!({ "mcpServers": { "comindly": serve } })),
            ));
            out.push((
                "hooks/hooks.json".into(),
                pretty(json!({ "hooks": {
                    "SessionStart": [{ "hooks": [{ "type": "command", "command": hook("session-start") }] }],
                    "PreToolUse": [{ "matcher": "Edit|MultiEdit|Write",
                        "hooks": [{ "type": "command", "command": hook("pre-edit") }] }],
                }})),
            ));
        }
        "gemini" => {
            out.push((
                "gemini-extension.json".into(),
                pretty(
                    json!({ "name": "comindly", "version": version, "description": DESCRIPTION,
                    "mcpServers": { "comindly": serve } }),
                ),
            ));
            out.push((
                "hooks/hooks.json".into(),
                pretty(json!({ "hooks": {
                    "SessionStart": [{ "hooks": [{ "type": "command", "command": hook("session-start") }] }],
                    "BeforeTool": [{ "matcher": "write_file|replace",
                        "hooks": [{ "type": "command", "command": hook("pre-edit") }] }],
                }})),
            ));
        }
        "codex" => {
            out.push((
                "plugin.json".into(),
                pretty(
                    json!({ "$schema": "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json",
                    "name": "comindly", "version": version, "description": DESCRIPTION }),
                ),
            ));
            out.push((
                "mcp.json".into(),
                pretty(
                    json!({ "$schema": "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json",
                    "mcpServers": { "comindly": match gateway {
                        Some(url) => json!({ "type": "http", "url": url }),
                        None => json!({ "type": "stdio", "command": exe, "args": ["serve"] }),
                    } } }),
                ),
            ));
        }
        "cursor" => {
            out.push((
                ".cursor-plugin/plugin.json".into(),
                pretty(
                    json!({ "name": "comindly", "version": version, "description": DESCRIPTION }),
                ),
            ));
            out.push((
                "mcp.json".into(),
                pretty(json!({ "mcpServers": { "comindly": serve } })),
            ));
            out.push((
                "rules/comindly.mdc".into(),
                format!(
                    "---\ndescription: When to use comindly's MCP tools\nalwaysApply: true\n---\n\n{}",
                    api::agents::GUIDE
                ),
            ));
        }
        other => anyhow::bail!("no plugin for `{other}`: claude, gemini, codex or cursor"),
    }
    Ok(out)
}

/// Write `agent`'s plugin under `dir`, replacing what an earlier version wrote there.
pub fn write(agent: &str, exe: &str, gateway: Option<&str>, dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).with_context(|| format!("clear {}", dir.display()))?;
    }
    for (path, text) in files(agent, exe, gateway)? {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().context("plugin path")?)?;
        std::fs::write(&p, text).with_context(|| format!("write {}", p.display()))?;
    }
    if agent == "claude" {
        // A local marketplace holding the one plugin, so `claude plugin install` can find it.
        let m = json!({ "name": "comindly", "owner": { "name": "comindly" },
            "plugins": [{ "name": "comindly", "source": "./comindly", "description": DESCRIPTION }] });
        let root = dir.parent().context("plugin dir")?;
        std::fs::create_dir_all(root.join(".claude-plugin"))?;
        std::fs::write(
            root.join(".claude-plugin/marketplace.json"),
            serde_json::to_string_pretty(&m)? + "\n",
        )?;
    }
    Ok(())
}

/// Where `--install` keeps the plugins it registers.
fn home_dir(agent: &str) -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".local/share")
        });
    match agent {
        "claude" => data.join("comindly/plugins/claude/comindly"),
        // Cursor loads plugins it finds here; a window reload picks them up.
        "cursor" => std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".cursor/plugins/local/comindly"),
        a => data.join("comindly/plugins").join(a),
    }
}

fn on_path(cmd: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// The agents found on this machine.
fn found() -> Vec<&'static str> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    AGENTS
        .iter()
        .copied()
        .filter(|a| match *a {
            "cursor" => home.join(".cursor").is_dir(),
            a => on_path(a),
        })
        .collect()
}

/// What the agents' comindly answers from: the local engine or a named server.
pub fn engine_in_use() -> String {
    if let Some(url) = std::env::var("COMINDLY_SERVER")
        .ok()
        .filter(|u| !u.is_empty())
    {
        return format!("the server at {url} (COMINDLY_SERVER)");
    }
    let name = crate::profile::chosen(None);
    if name == crate::profile::LOCAL {
        return "the local engine on this machine".into();
    }
    match crate::profile::Profiles::load().profiles.get(&name) {
        Some(p) => format!("the team server `{name}` ({})", p.url),
        None => format!("the profile `{name}`, which is not set up"),
    }
}

/// The gateway of the team server in use, which agents connect through instead of `serve`.
fn gateway() -> Option<String> {
    if crate::profile::chosen(None) == crate::profile::LOCAL {
        return None;
    }
    let c = crate::profile::client(None).ok()?;
    match crate::commands::block(c.get::<api::Me>("/api/v1/auth/me", &())) {
        Ok(api::Me::Server { gateway_url, .. }) => gateway_url.filter(|u| !u.is_empty()),
        _ => None,
    }
}

/// Each step `--install` takes for `agent`, as commands to run (empty: nothing to run).
fn steps(agent: &str, dir: &Path) -> Vec<Vec<String>> {
    let d = dir.display().to_string();
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match agent {
        "claude" => {
            let market = dir
                .parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            vec![
                s(&["claude", "plugin", "marketplace", "add", &market]),
                s(&["claude", "plugin", "marketplace", "update", "comindly"]),
                s(&["claude", "plugin", "install", "comindly@comindly"]),
            ]
        }
        "gemini" => vec![s(&["gemini", "extensions", "link", &d])],
        _ => Vec::new(),
    }
}

/// `comindly init-agents --plugin <agent> --out <dir>`.
pub fn print_plugin(agent: &str, out: &Path) -> ExitCode {
    let dir = if agent == "claude" {
        out.join("comindly")
    } else {
        out.to_path_buf()
    };
    match write(agent, "comindly", None, &dir) {
        Ok(()) => {
            crate::term::ok(&format!(
                "wrote comindly's {agent} plugin to {}",
                out.display()
            ));
            ExitCode::SUCCESS
        }
        Err(e) => {
            crate::term::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

/// `comindly init-agents --install [--yes]`: write and register the plugin for each agent found.
pub fn install(yes: bool) -> ExitCode {
    let agents = found();
    if agents.is_empty() {
        crate::term::note("no Claude Code, Gemini CLI, Codex or Cursor found on this machine");
        return ExitCode::SUCCESS;
    }
    let gateway = gateway();
    if let Some(url) = &gateway {
        crate::term::note(&format!(
            "the team server takes agents through its gateway, {url}: they sign in there"
        ));
    }
    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "comindly".into());
    let mut failed = false;
    for agent in agents {
        let dir = home_dir(agent);
        let run = steps(agent, &dir);
        if !yes {
            crate::term::note(&format!(
                "{agent}: would write its plugin to {}",
                dir.display()
            ));
            for cmd in &run {
                println!("  {}", cmd.join(" "));
            }
            continue;
        }
        if let Err(e) = write(agent, &exe, gateway.as_deref(), &dir) {
            crate::term::err(&format!("{agent}: {e:#}"));
            failed = true;
            continue;
        }
        for cmd in &run {
            let ok = Command::new(&cmd[0])
                .args(&cmd[1..])
                .output()
                .is_ok_and(|o| o.status.success());
            // Adding a marketplace that is already there fails; the update after it covers that.
            if !ok && !(agent == "claude" && cmd.get(3).is_some_and(|c| c == "add")) {
                crate::term::err(&format!("{agent}: `{}` failed", cmd.join(" ")));
                failed = true;
            }
        }
        match agent {
            "codex" => crate::term::note(&format!(
                "codex: plugin written to {}; add it in Codex with /plugins",
                dir.display()
            )),
            "cursor" => crate::term::ok("cursor: plugin installed; reload the Cursor window"),
            a => crate::term::ok(&format!("{a}: plugin installed; start a new session")),
        }
    }
    if !yes {
        crate::term::note("add --yes to do this");
    }
    crate::term::note(&format!(
        "agents will use {}; switch with `comindly use`",
        engine_in_use()
    ));
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_plugin_carries_the_skill_and_the_server_and_names_no_address() {
        for agent in AGENTS {
            let files = files(agent, "comindly", None).unwrap();
            let all: String = files.iter().map(|(_, t)| t.as_str()).collect();
            assert!(
                files
                    .iter()
                    .any(|(p, t)| p.ends_with("SKILL.md") && *t == api::agents::skill()),
                "{agent}: the skill is the one init-agents --skill prints"
            );
            assert!(all.contains("\"serve\""), "{agent}: starts comindly serve");
            let rest = all.replace("https://agent-plugins.org/", "");
            assert!(!rest.contains("://"), "{agent}: names no server");
            for (p, t) in &files {
                if p.extension().is_some_and(|e| e == "json") {
                    serde_json::from_str::<serde_json::Value>(t)
                        .unwrap_or_else(|e| panic!("{agent} {}: {e}", p.display()));
                }
            }
        }
    }

    #[test]
    fn hooks_run_the_same_commands_init_hooks_prints() {
        let claude = files("claude", "comindly", None).unwrap();
        let hooks = &claude
            .iter()
            .find(|(p, _)| p.ends_with("hooks.json"))
            .unwrap()
            .1;
        assert!(hooks.contains("comindly hook claude session-start"));
        assert!(hooks.contains("comindly hook claude pre-edit"));
        let gemini = files("gemini", "comindly", None).unwrap();
        let hooks = &gemini
            .iter()
            .find(|(p, _)| p.ends_with("hooks.json"))
            .unwrap()
            .1;
        assert!(hooks.contains("comindly hook gemini session-start"));
        assert!(files("windsurf", "comindly", None).is_err());
    }

    #[test]
    fn behind_a_gateway_agents_connect_there() {
        for agent in AGENTS {
            let all: String = files(agent, "comindly", Some("https://mcp.example.com/mcp"))
                .unwrap()
                .into_iter()
                .map(|(_, t)| t)
                .collect();
            assert!(all.contains("https://mcp.example.com/mcp"), "{agent}");
            assert!(!all.contains("\"serve\""), "{agent}: no local serve");
        }
    }

    #[test]
    fn a_rewrite_replaces_the_old_plugin() {
        let dir = std::env::temp_dir().join(format!("comindly-plugin-{}", std::process::id()));
        let plugin = dir.join("comindly");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("stale.txt"), "old").unwrap();
        write("claude", "/usr/local/bin/comindly", None, &plugin).unwrap();
        let mcp = std::fs::read_to_string(plugin.join(".mcp.json")).unwrap();
        let market = dir.join(".claude-plugin/marketplace.json").exists();
        let stale = plugin.join("stale.txt").exists();
        std::fs::remove_dir_all(&dir).ok();
        assert!(mcp.contains("/usr/local/bin/comindly"));
        assert!(market && !stale);
    }
}
