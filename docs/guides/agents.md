# Using comindly with agents

An agent connected to comindly (see [Getting started](../getting-started.md#4-connect-your-agent), or on
a team server Settings → Your access, which also covers signing in through your organisation's
[gateway](self-hosting.md#agents-through-a-gateway) with no token)
sees fifteen tools. It decides when to call them; you ask in plain words. This page shows what to
ask, what happens, and the habits that keep the answers good.

## What to ask

| You want to know | Ask, for example | Tool the agent uses |
|---|---|---|
| Where something lives | "Where do we retry outgoing HTTP calls?" | `search` |
| A symbol by name | "Find the `OrderService` class." | `find` |
| Everything around a symbol | "Show me what calls and what is called by `place_order`." | `zoom` |
| What a change can break | "What breaks in other services if I change `place_order`'s signature?" | `ripple` |
| What to read before a change | "What should I read before changing `PaymentClient.charge`?" | `context_pack` |
| How a request flows | "Trace what happens after `place_order` is called." | `thread`, `flow` |
| Which services talk, and how | "Who calls `GET /orders/{id}`? Which queue does orders publish to?" | `contracts` |
| How the repositories fit together | "Which repositories are indexed and who depends on whom?" | `repos` |
| How code is written here | "Which conventions should I follow in the payments repo?" | `guide` |
| Why code is the way it is | "Why is this lock here? Did anyone try something else?" | `why` |
| Whether a planned change is safe | "I am about to change `auth.py` and `errors.py`. Anything I should know?" | `preflight` |
| Ideas for what to ask | "What can I ask about the payments module?" | `suggest` |

The agent can also write to the team's ledger:

| You want to | Say | Tool |
|---|---|---|
| Record a decision, dead end or rule | "Remember that a TTL of `None` means cache entries never expire." | `remember` |
| Retire or confirm a record | "That constraint about pinning the client no longer applies: archive it." | `review` |

Names in answers are the ones you see in the UI: methods as `Class.method`, contracts as
`http:GET /orders/{}` or `sqs:order-events`, locations as `path:line`.

## How a good session looks

1. **Before editing**, the agent runs `preflight` (or `zoom` and `why`) for the files or symbols
   it plans to touch. It learns the callers in other repositories, the contracts the code serves,
   and the constraints and failed attempts recorded about it.
2. **While editing**, the [hooks](#hooks) put the records about each file in front of the agent
   just before it writes to it.
3. **After a surprise** (a reverted approach, an invariant nobody wrote down, a trap) the agent
   records it with `remember`, so the next person or agent does not repeat it.

To nudge an agent that does not reach for comindly, name it: "use comindly to check what depends on
this first".

## Telling agents when to use it

comindly sends agents guidance on when to reach for each tool: as the MCP server's instructions when
they connect, and at the start of each session through the [hooks](#hooks). To put the same guidance
in a repository, where agents read it before they look at their tools and your team can adjust it:

```sh
comindly init-agents >> AGENTS.md          # or CLAUDE.md
comindly init-agents --skill               # a Claude Code skill for .claude/skills/comindly/SKILL.md
```

The app's Get started page shows both, ready to copy.

![Get started: the guidance for agents, as an AGENTS.md section or a Claude Code skill](../assets/screenshots/setup-agents.webp#only-light)
![Get started: the guidance for agents, as an AGENTS.md section or a Claude Code skill](../assets/screenshots/setup-agents-dark.webp#only-dark)

## Hooks

`comindly init-hooks claude` prints two hooks for Claude Code:

- **Session start**: when the repository you are working in is indexed, the guidance on when to use
  comindly's tools, then up to eight trusted records about the repository.
- **Before an edit** (`Edit`, `MultiEdit`, `Write`): up to four trusted records about that file.

A record is shown once per session. Hooks read a local mirror of a remote index, refreshed every
five minutes (`cache_ttl_secs`), so they add about 60 ms. For other agents, `comindly init-hooks
cursor`, `gemini`, `codex` or `generic` prints the two commands to run from the agent's own hooks;
they print the same context as plain markdown.

## The ledger, from an agent's side

Records are short, typed and anchored to symbols: `decision`, `attempt` (a failed or reverted
approach, with evidence), `constraint`, `gotcha`, `intent` and `question`.

- Every record shows who wrote it: the person, or the agent and the person it works for
  ("claude-code for Ada Lovelace"), and when.
- An agent's record without evidence starts as **unverified** and appears in the Ledger page's
  review queue; with evidence (a commit, a test, an MR) it is trusted enough for hooks.
- Agents can confirm, archive, invalidate, supersede or dispute records. When an agent tries to
  retire a record a person wrote or confirmed, the record stays and is flagged for a person.
- Deleting a record is for people only, from the web UI or `comindly erase`.

Good records are neutral facts a future engineer would want before editing, not summaries of a
session. comindly refuses records that look like secrets or that try to give an agent instructions.
See [The ledger](ledger.md) for trust levels and review.

## Conventions

`guide` returns the conventions comindly measured in the code (naming, error base classes, where
HTTP clients live, which internal library wraps `requests`, how tests mock) and the ones the team
added or edited. The team's own are marked "set by the team". Edit them on the Conventions page
or with `comindly conventions add|edit|rm|restore`.

## When something is off

| Symptom | Check |
|---|---|
| The agent lists no comindly tools | Restart the agent after adding the server; run the `comindly serve` command yourself to see errors. |
| "no index here yet" | The `--index-dir` path; `comindly doctor --index-dir …`. |
| An index in a bucket fails to load | The server's environment needs storage credentials ([Storage](storage.md)). |
| `search` finds nothing | The index was built with `--no-embed`; rebuild without it. |
| Answers miss recent code | The index is older than the code: re-run `comindly link` (CI usually does). |
| New conventions or records do not show | A running MCP server loads them at start; restart the agent. |
