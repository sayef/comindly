# MCP tools

`comindly serve --index-dir <dir>` speaks MCP over stdio; a team server also serves the tools over HTTP
at `/mcp` ([Self-hosting](../guides/self-hosting.md#agents)). Results are markdown by default, with
structured JSON attached (`--format json` for JSON only). How to register the server with an agent:
[Getting started](../getting-started.md#4-connect-your-agent); what to ask:
[Using comindly with agents](../guides/agents.md).

Over HTTP, an agent works with the role of the person whose token it uses. A viewer's agent can call
every reading tool, but `remember` and `review` are refused; a member's or an admin's agent can also
record and review.

Wherever a tool takes `focus`, it accepts a symbol name (`place_order`), a qualified method
(`OrderService.place_order`), a file path, or an id returned by `find` or `search`.
Optional parameters are marked with `?`.

## Finding code

| Tool | Parameters | Returns |
|---|---|---|
| `search` | `query`, `limit?` (12) | Graph-aware hybrid search over names, code and summaries (BM25, vectors, centrality) |
| `find` | `query`, `limit?` (20) | Symbols whose name or path contains `query`, with ids for the other tools |
| `repos` | | Indexed repositories, their sizes and internal package dependencies |
| `suggest` | `about?`, `limit?` (40) | Questions the index can answer, optionally about one keyword or symbol |

## Understanding a symbol

| Tool | Parameters | Returns |
|---|---|---|
| `zoom` | `focus` | Callers, callees, importers, members, contract roles, and the constraints and dead ends recorded about it |
| `thread` | `focus`, `depth?` (4) | Forward call trace: what it calls, transitively |
| `flow` | `focus`, `depth?` (4) | A pre-generated walkthrough of an entry point (when built with flows) plus its live call trace |
| `context_pack` | `focus`, `token_budget?` (1500) | The smallest set of symbols to read before changing it, within the token budget |

## Impact and safety

| Tool | Parameters | Returns |
|---|---|---|
| `ripple` | `focus`, `depth?` (4) | Blast radius: everything that depends on it, across repositories and contracts, by repository |
| `contracts` | `kind?` (`http`, `sqs`, `sns`, `s3`, `table`), `query?`, `dangling?` | Routes, queues, topics, buckets and tables with providers and consumers; `dangling` lists the ones missing a side |
| `preflight` | `files?`, `symbols?` | For planned edits: recorded constraints and dead ends, contracts other services use, blast radius, files that usually change together, failing checks |

## Team knowledge

| Tool | Parameters | Returns |
|---|---|---|
| `guide` | `focus?` | Measured conventions and change recipes for a repository (from a repo, file or symbol), the org baseline, and the team's own conventions |
| `why` | `focus` | Recorded decisions, failed attempts, constraints and gotchas about it, with who, when, status and trust |
| `remember` | `kind`, `on`, `title`, `body?`, `evidence?`, `trigger?`, `fields?` | Records a `decision`, `attempt` (evidence required), `constraint`, `gotcha`, `intent` or `question`, anchored to the symbols in `on` |
| `review` | `id`, `action`, `reason?` | `confirm`, `archive` (no longer applies), `invalidate` (was wrong), `supersede` or `dispute` a record; retiring one a person wrote or confirmed only flags it for a person |

Records written through MCP name the agent (from the MCP client, or `COMINDLY_AGENT`) and the person
it works for (config `author_name`, else git `user.name`; on a team server, the member whose token the
agent uses). Deleting records is not available over
MCP.
