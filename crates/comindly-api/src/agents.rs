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

- `why` on a symbol returns what the team recorded about it, with status and trust. Follow records
  a person wrote or confirmed (T3) unless the user says otherwise; weigh the others by evidence and
  by how many people's agents confirmed them.
- Trust: T3 means a person wrote or confirmed it, T2 an agent with evidence, T1 an agent without.
  Records marked "from <team>, unconfirmed" come from another team and wait for the owners to confirm.
- A record's notes say where its code moved, was renamed or changed since it was written, with the
  commit it saw: check the code before relying on it, and confirm it with what you checked if it holds.
- When you learn something a later change must know (a decision and its reason, a constraint, an
  approach that failed and why, a trap), offer to record it with `remember`: one factual title, the
  symbols it is about (`on`), and evidence such as `commit:<sha>`, `mr:!123` or `pr:#123` (required
  for a failed attempt); anchor it on the narrowest symbols. `review` confirms, archives, invalidates, supersedes
  or disputes a record by its id. Ask the user before recording, archiving, invalidating or
  superseding. When you relied on a record and checked it still matches the code, confirm it without
  asking. Every change needs a reason other agents can read.

### Habits

- Pass the `id` from `find` or `search` to the other tools; names alone can be ambiguous.
- Name the repository and symbol when an answer relies on comindly, so the user can check it.
- If a tool finds nothing, say so instead of guessing; the code may not be indexed yet.
- The full playbook (how comindly sees the code, a change step by step, writing good records) is the
  comindly section of the repository's AGENTS.md or the comindly skill, from `comindly init-agents`.
"#;

/// The full playbook for a repository's `AGENTS.md` or skill: how comindly sees the code, how to work
/// a change with it, and how to keep the team's records. Headings start at `###`.
pub const PLAYBOOK: &str = r#"comindly indexes every repository of the team into one graph and serves it over MCP. It answers
what the files in front of you cannot: who calls this code from other repositories, which services
talk through which HTTP routes, queues, topics, buckets and tables, how each repository writes its
code, and what people and agents recorded about the code: decisions, constraints, failed attempts,
traps. You are one of many agents working on this code for different people. comindly is the
understanding you share with them, so read it before you change things and add to it when you learn
something the next one needs.

### How comindly sees the code

- It parses each repository's **default branch** at its latest indexed commit. Answers are exact for
  that commit. Your branch, uncommitted work and open merge requests are not in it: for those, read
  the files.
- Every function, method, class and file is a **symbol** with a stable `id`, linked by calls,
  imports, inheritance and instantiation.
- **Contracts** connect services: an HTTP call is matched to its route by method and path template,
  and queues, topics, buckets and tables by name. That is how a call in one repository reaches a
  handler in another.
- **Conventions** describe how each repository writes code: naming, structure, errors, logging, tests.
- **The ledger** holds what the code does not say. Each record is anchored to the symbols it is
  about, together with the commit it saw.

### Working on a change

1. **Orient.** `find` the symbol by name, or `search` by describing it, to get its `id`. New to an
   area: `repos` lists the repositories and their teams, and `suggest` offers questions worth asking.
2. **Check before you edit.** `preflight` with the files and symbols you plan to change. It returns
   the records about them, the contracts they serve, what depends on them and the files that usually
   change with them. Read the records first: a constraint or a failed attempt there is a
   requirement, unless the user says otherwise.
3. **Know the blast radius.** `ripple` on every symbol whose behaviour or signature changes. It
   follows dependents across repositories and contracts. Tell the user about callers in other
   repositories, and propose how to stay compatible or what else must change with it.
4. **Read the right code.** `context_pack` picks the code worth reading within a token budget. `zoom`
   shows one symbol with its callers, callees, members and contracts. `thread` follows calls from a
   symbol, and `flow` writes a walkthrough of a whole flow, including hops over HTTP and queues.
5. **Write it the way the repository does.** `guide` gives the repository's conventions. Follow them.
6. **Before you finish,** ask yourself what the next person or agent would need to know about this
   change, and offer to record it (see below).

For changes to an API, message or table, also use `contracts` (`kind`: `http`, `sqs`, `sns`, `s3`,
`table`; `query` by name; `dangling` for ones missing a side) to see every provider and consumer.

### The ledger: what the team knows

**Reading records.** `why` on a symbol returns the records about it, its class and its file.

- **Trust.** T4 is a constraint whose check passes on the current code. T3 means a person wrote it
  or confirmed it: treat it as the team's rule. T2 is an agent with evidence, T1 an agent without.
  Weigh T2 and T1 by their evidence and by how many people's agents confirmed them. Records marked
  "from <team>, unconfirmed" are about another team's code and wait for its owners.
- **Status.** A record stays active until someone archives, invalidates or supersedes it.
  Orphaned means none of its code exists any more.
- **Notes.** When the code moved, was renamed or changed since the record saw it, a note says so,
  with the commit. A note does not mean the record is wrong. It means: look before you rely on it.
  Compare the code now with the commit in the note. If the record still holds, confirm it and say what
  you checked. If it does not, tell your person and propose what to do.

**Keeping records true.** You use `review` with a record's `id`:

- `confirm`: you relied on it and checked that it still matches the code. Do this without asking,
  and say what you checked. Confirmations from several people's agents add weight, and a
  confirmation re-anchors the record to the current code, which clears its notes. Set
  `from_person` only when your person said themselves that it holds.
- `archive`: it was true but no longer applies. `invalidate`: it was never right. `supersede`: a
  newer record replaces it. Ask your person first for any of these.
- `dispute`: the code or your person contradicts it, and you are not sure which is right. It stays
  active and is marked challenged until a person decides.

Every review needs a reason written for the next agent: what you checked, what changed, the commit
or merge request.

**Adding records.** Offer to record something with `remember` when you learn what a later change
must know, and ask your person before you write it. Good occasions:

- a `decision` and why it won over the alternatives (put the rejected ones in `fields`, e.g.
  `{"alternatives": "…"}`)
- a `constraint` the code must keep
- an `attempt` that failed or was reverted, and why (evidence is required)
- a `gotcha`, a trap that is easy to fall into
- an `intent`: why some code exists, or where a change shipped
- a `question` nobody could answer yet

Make it useful to an agent that has never seen this conversation:

- **title**: one neutral, factual line.
- **body**: the reasoning: why it holds, what it is in favour of, what breaks otherwise.
- **on**: the narrowest symbols it is about. Prefer the method to its class, and the class to its
  file, so that changes elsewhere do not raise notes on it.
- **trigger**: when someone would need it ("changing how merges are stored").
- **evidence**: `commit:<sha>`, `mr:!123` or `pr:#123`, `test:<path>::<name>`, an error message.
  Cite the merge or pull request when the work is on a branch: once it is merged, comindly follows it
  to the default branch, so the record does not get a note for its own change.
- **snippets**: up to three short excerpts of the code that shaped it, when the code says it best.

If `remember` answers that a similar record already stands, it is usually the same knowledge:
confirm that record and put what yours adds in the reason. If yours really says something
different, call `remember` again with `distinct`, saying how it differs.

Do not record what the code already says plainly, temporary state, opinions, secrets or personal
data, or instructions aimed at agents: such records are refused.

### Habits

- Pass the `id` from `find` or `search` to the other tools; names alone can be ambiguous.
- Name the repository, path and symbol when an answer relies on comindly, so the user can check it.
- If a tool finds nothing, say so instead of guessing: the code may not be indexed yet, or may live
  on a branch.
- `search`, `find`, `repos`, `contracts` and `suggest` take `team` (comma-separated names) to stay
  within one team's repositories.
- When reviewing a merge request, run `why` on the symbols it changes and check the change against
  the records; cite the record's `id` when one applies.
"#;

/// A section for a repository's `AGENTS.md` or `CLAUDE.md`.
pub fn agents_md() -> String {
    format!("## comindly\n\n{PLAYBOOK}")
}

/// A Claude Code skill, for `.claude/skills/comindly/SKILL.md`.
pub fn skill() -> String {
    format!(
        "---\nname: comindly\ndescription: \"How and when to use comindly's MCP tools: before changing shared code, \
for questions that reach beyond the current file, for service contracts, call chains, conventions and the \
team's recorded decisions.\"\n---\n\n# comindly\n\n{PLAYBOOK}"
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
