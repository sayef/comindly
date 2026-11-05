//! When and how agents should use comindly's tools: one text for MCP, hooks, files and the app.

/// The guidance itself, as markdown without a heading.
pub const GUIDE: &str = "\
This codebase is indexed by comindly: code intelligence across all of the team's repositories. \
Use its MCP tools instead of grepping or guessing whenever a question reaches beyond the file in front of you.

- Before changing a function, class, route or schema: `ripple` for what depends on it in every repository, \
`preflight` for recorded constraints, affected contracts and companion files, `context_pack` for what to read first.
- Before editing code with history: `why` for the decisions, constraints and failed attempts the team recorded.
- To find code: `find` by name, `search` by describing it; then `zoom` on the returned `id` for its definition, \
callers, callees and members.
- To follow behaviour: `thread` for the call chain from a symbol, `flow` for a narrated walkthrough, \
`contracts` for who provides and consumes an HTTP route, queue, topic, bucket or table.
- To match the codebase: `guide` for the conventions measured in each repository.
- To get oriented: `repos` for what is indexed, `suggest` for questions worth asking. \
`search`, `find`, `repos`, `contracts` and `suggest` take `team` to stay within one team's repositories.
- When you learn something a later change must know (a decision, a constraint, a dead end), offer to record it \
with `remember`; `review` confirms or retires records. Ask the user before writing either.

Pass the `id` that `find` or `search` returns to the other tools. \
Name the repository and symbol when an answer relies on comindly.
";

/// A section for a repository's `AGENTS.md` or `CLAUDE.md`.
pub fn agents_md() -> String {
    format!("## comindly\n\n{GUIDE}")
}

/// A Claude Code skill, for `.claude/skills/comindly/SKILL.md`.
pub fn skill() -> String {
    format!(
        "---\nname: comindly\ndescription: \"Use comindly's MCP tools for questions about this codebase that reach \
beyond the current file: what depends on code before changing it, recorded decisions, service contracts, \
call chains and conventions.\"\n---\n\n# comindly\n\n{GUIDE}"
    )
}
