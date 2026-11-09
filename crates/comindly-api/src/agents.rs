//! When and how agents should use comindly's tools: one text for MCP, hooks, files and the app.

/// The guidance itself, as markdown whose headings start at `###`.
pub const GUIDE: &str = r#"comindly indexes all of the team's repositories into one graph and serves it over MCP. It knows
what this file cannot tell you: who calls code in other repositories, which services talk through
which HTTP routes, queues, topics, buckets and tables, how each repository writes its code, and what
the team recorded about the code (decisions, constraints, failed attempts, traps). Its answers come
from parsing the code, so they are exact for the indexed version.

### When to use it

- Before changing a function, class, route, message or table that anything else might use.
- When a question reaches beyond the files you have open: "who calls this", "where is this
  handled", "what consumes this queue", "how does a request get from A to B".
- Before editing code that looks odd or fragile: someone may have recorded why it is that way, or
  what already failed.
- When you need to write code the way this codebase does: naming, structure, logging, tests.
- When you are new to an area and need to find your way.

Do not use it for questions the open files answer, or for code that is not committed yet: the
index reflects each repository's default branch as of the last indexing.

### How to work with it

**Before changing code**
1. `find` the symbol by name (or `search` by describing it) to get its `id`.
2. `preflight` with the files and symbols you plan to change: recorded constraints and dead ends,
   the contracts the code serves, what depends on it, and companion files that usually change with it.
3. `ripple` on each symbol whose behaviour or signature changes: everything that depends on it, in
   every repository, across service contracts. Mention callers in other repositories to the user.
4. `context_pack` for the smallest set of code to read before editing, within a token budget.
5. `guide` for the repository's conventions, and follow them.

**Understanding behaviour**
- `zoom` on an `id`: its definition, callers, callees, members and contracts in one answer.
- `thread` follows the calls from a symbol; `flow` gives a written walkthrough of a whole flow,
  including hops over HTTP and queues into other services.
- `contracts` lists routes, queues, topics, buckets and tables with who provides and who uses each;
  `kind` narrows to `http`, `sqs`, `sns`, `s3` or `table`, `query` filters by name, `dangling` shows
  ones missing a side.

**Finding your way**
- `repos` lists the indexed repositories, their size and teams; `suggest` offers questions worth
  asking about an area (`about`).
- `search`, `find`, `repos`, `contracts` and `suggest` take `team` (comma-separated names) to stay
  within one team's repositories.

### The team's records

- `why` on a symbol returns what the team recorded about it, with status and trust. Treat active
  records as requirements unless the user says otherwise.
- Trust: T3 means a person wrote or confirmed it, T2 an agent with evidence, T1 an agent without.
  Records marked "from <team>, unconfirmed" come from another team and wait for the owners to confirm.
- Stale or moved records point at code that changed since they were written: check them against the
  code before relying on them.
- When you learn something a later change must know (a decision and its reason, a constraint, an
  approach that failed and why, a trap), offer to record it with `remember`: one factual title, the
  symbols it is about (`on`), and evidence such as `commit:<sha>` or `mr:!123` (required for a failed
  attempt). `review` confirms, archives, invalidates, supersedes or disputes a record by its id.
  Ask the user before writing either.

### Habits

- Pass the `id` from `find` or `search` to the other tools; names alone can be ambiguous.
- Name the repository and symbol when an answer relies on comindly, so the user can check it.
- If a tool finds nothing, say so instead of guessing; the code may not be indexed yet.
"#;

/// A section for a repository's `AGENTS.md` or `CLAUDE.md`.
pub fn agents_md() -> String {
    format!("## comindly\n\n{GUIDE}")
}

/// A Claude Code skill, for `.claude/skills/comindly/SKILL.md`.
pub fn skill() -> String {
    format!(
        "---\nname: comindly\ndescription: \"How and when to use comindly's MCP tools: before changing shared code, \
for questions that reach beyond the current file, for service contracts, call chains, conventions and the \
team's recorded decisions.\"\n---\n\n# comindly\n\n{GUIDE}"
    )
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
