//! Setup checks shared by `comindly doctor` and the web UI's setup page.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub ok: bool,
    /// Optional extras (signing, SCIP) do not count as problems.
    pub optional: bool,
    pub message: String,
}

/// Check the index, ledger, remote access, hooks, LLM, signing and SCIP indexers for `root`.
pub fn run(
    cfg: &crate::config::Config,
    team: &crate::team::TeamSettings,
    server: Option<&crate::server::Server>,
    root: &str,
) -> Vec<Check> {
    let resolved = crate::context::Settings::resolve(team, cfg);
    let set = |k: &str| std::env::var(k).is_ok_and(|v| !v.is_empty());
    let mut out = Vec::new();
    let mut check = |good: bool, optional: bool, ok: &str, bad: &str| {
        out.push(Check {
            ok: good,
            optional,
            message: if good { ok } else { bad }.to_string(),
        })
    };
    let file = std::fs::read_to_string(crate::config::config_path()).unwrap_or_default();
    let obsolete: Vec<&str> = ["aws_profile", "sso_auto_login"]
        .into_iter()
        .filter(|k| {
            file.lines().any(|l| {
                l.trim_start()
                    .strip_prefix(k)
                    .is_some_and(|r| r.trim_start().starts_with('='))
            })
        })
        .collect();
    if !obsolete.is_empty() {
        check(
            false,
            true,
            "",
            &format!(
                "config.toml still sets {}, which comindly no longer reads: remove it and set AWS_PROFILE in the environment if you use a profile",
                obsolete.join(" and ")
            ),
        );
    }
    let graph = format!("{}/_graph", root.trim_end_matches('/'));
    match crate::index::graph_versions_blocking(&graph) {
        Ok(v) if !v.is_empty() => {
            let (_, _, at) = v.last().cloned().unwrap_or_default();
            let msg = format!(
                "index readable: {} version(s), latest {}",
                v.len(),
                at.get(..16).unwrap_or(&at)
            );
            check(true, false, &msg, "");
        }
        _ => check(
            false,
            false,
            "",
            "no index here yet: run `comindly link <repos> --index-dir …`",
        ),
    }
    match crate::ledger::all_blocking(root) {
        Ok(r) => check(
            true,
            false,
            &format!("ledger readable: {} record(s)", r.len()),
            "",
        ),
        Err(e) => check(false, false, "", &format!("ledger not readable: {e:#}")),
    }
    if crate::cache::is_remote(root) {
        let fresh = crate::cache::dir(root).is_some_and(|d| d.join("_graph").is_dir());
        check(
            fresh,
            false,
            "local mirror present (hooks read it)",
            "no local mirror yet: `comindly cache refresh` (hooks create it on first use)",
        );
    }
    let settings = std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".claude/settings.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let local = std::fs::read_to_string(".claude/settings.json").unwrap_or_default();
    check(
        settings.contains("comindly hook") || local.contains("comindly hook"),
        true,
        "Claude Code hooks configured",
        "Claude Code hooks not found: `comindly init-hooks claude`",
    );
    check(
        set("OPENAI_API_KEY")
            || resolved.llm_base_url.is_some()
            || crate::credentials::llm_key(team.name.as_deref(), server)
                .0
                .is_some(),
        true,
        &format!("LLM configured (model {})", resolved.llm_model),
        "no LLM key: summaries, flows and style guides are off (indexing works without)",
    );
    check(
        crate::config::local_signing_key().is_some(),
        true,
        "ledger records are signed",
        "records are unsigned (optional: `signing_key` or git SSH signing)",
    );
    check(
        crate::config::local_allowed_signers().is_some(),
        true,
        "signatures are verified against allowed_signers",
        "no allowed_signers: signatures are not checked (optional)",
    );
    let indexers: Vec<&str> = [
        "scip-python",
        "scip-typescript",
        "scip-go",
        "rust-analyzer",
        "scip-java",
    ]
    .into_iter()
    .filter(|b| crate::scip::which(b).is_some())
    .collect();
    check(
        !indexers.is_empty(),
        true,
        &format!("SCIP indexers on PATH: {}", indexers.join(", ")),
        "no SCIP indexers on PATH (optional: compiler-grade edges)",
    );
    out
}

/// Agents on this machine whose MCP settings already list comindly.
pub fn agents() -> Vec<&'static str> {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else {
        return Vec::new();
    };
    let json = |p: &str| -> serde_json::Value {
        std::fs::read_to_string(home.join(p))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    };
    let has = |v: &serde_json::Value| {
        v.get("mcpServers")
            .and_then(|m| m.get("comindly"))
            .is_some()
    };
    let claude = json(".claude.json");
    let mut out = Vec::new();
    if has(&claude)
        || claude["projects"]
            .as_object()
            .is_some_and(|p| p.values().any(has))
    {
        out.push("claude");
    }
    if has(&json(".cursor/mcp.json")) {
        out.push("cursor");
    }
    if std::fs::read_to_string(home.join(".codex/config.toml"))
        .is_ok_and(|t| t.contains("[mcp_servers.comindly]"))
    {
        out.push("codex");
    }
    out
}
