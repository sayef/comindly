# Command line

`comindly` never opens an index itself: every command asks an engine's `/api/v1`. The engine is the
one on this machine (`local`) or a team server, chosen with profiles.

## Profiles

| Command | Purpose |
|---|---|
| `comindly use` | List the profiles; `*` marks the one in use. `local` always exists |
| `comindly use local` | Use the engine on this machine. It starts in the background when a command needs it, and is installed on first use when missing |
| `comindly use <https-url> <name>` | Add a team server under a name, switch to it and sign in (approve in the browser) |
| `comindly use <name>` | Switch to a profile |
| `comindly login` / `comindly logout` | Sign the current profile in again, or sign it out: revoke the token on the server and forget the sign-in |
| `comindly engine start\|stop\|status\|install` | The local engine: start it in the background, stop it, show whether it runs and where, or download `comindly-engine` of this version from the GitHub release and put it beside `comindly` |

`--use <name>` on any command runs just that command against another profile; `COMINDLY_PROFILE` does
the same for every command in a shell. In CI, `COMINDLY_SERVER` and `COMINDLY_TOKEN` stand in for a
profile. Profiles and their sign-ins are kept in `~/.config/comindly/profiles.toml`, readable only by
you. A sign-in from an earlier version becomes a profile called `default`.

## Commands

| Command | Purpose |
|---|---|
| `comindly index <folder> [--no-llm]` | Index one folder as it is (local engine only) |
| `comindly link [folders…] [--no-llm] [--rewrite summary,guide,flow]` | Index into one cross-repo index (cross-repo edges and blast radius). With folders (local engine only), those folders as they are; without, the engine's repository list. `--no-llm` skips the LLM steps; `--rewrite` writes those LLM results again for all code instead of reusing them |
| `comindly explore <focus>` | Zoom, blast radius, and context pack for a symbol |
| `comindly search <query…> [--repo <name>] [--team <name>] [--format table]` | Graph-aware hybrid code search |
| `comindly find <query> [--repo <name>…] [--team <name>…]` | Locate symbols by name/path substring |
| `comindly flow <focus>` | Pre-generated flow walkthrough + live call trace |
| `comindly stats [--repo <name>…]` | Per-repo stats: symbols, edges, kinds, enrichment coverage |
| `comindly guide [--repo <name>…] [-o <file>]` | Per-repo inferred coding style guides |
| `comindly changed <repo> [--since <sha>]` | Files changed since a commit (git diff) |
| `comindly contracts [--type http\|sqs\|sns\|s3\|table] [--dangling]` | Service contracts with providers and consumers (FastAPI, Next.js, Express, Spring MVC, Feign, RestTemplate, WebClient, Go net/http/gin/echo/chi, axum, actix, reqwest, httpx/requests/fetch/axios, boto3, SQL; Terraform queues/topics/buckets and subscriptions) |
| `comindly packages [--skew]` | Internal packages: publisher, consumers, version skew |
| `comindly conventions [--repo <name>]` | Measured conventions, org baseline, recipes from history, and the team's own (each with an id) |
| `comindly conventions add "<rule>" [--repo <name>] [--dimension <d>] \| edit <id> "<rule>" \| rm <id> \| restore <id>` | Add a team convention, rewrite or hide one (measured ones can be restored); stored under `<index>/_conventions/`, untouched by re-indexing |
| `comindly preflight [repo] [--base <rev>] [--strict]` | Before committing or in MR CI: constraints, dead ends, failing checks, contracts, blast radius, missing companions ([MR job](../guides/ci.md#merge-request-preflight)) |
| `comindly remember <kind> --on <symbol> --title …` | Record a decision, failed attempt, constraint, gotcha, intent or question |
| `comindly why <symbol>` | Reasoning blame for a symbol (status, trust, evidence) |
| `comindly discussion open --repo <name> [--ticket <t>] [--branch <b>] --title … \| post <id> --body … [--kind <k>] \| read <id> \| list [--repo …] [--ticket …] [--branch …] [--query …]` | Working notes on a task or pull request, kept apart from the ledger; stored under `<index>/_discussions/` |
| `comindly review <id> <confirm\|archive\|invalidate\|supersede\|dispute> [--reason "…"]` | Act on a ledger record; `comindly erase <id>` deletes one |
| `comindly ledger [--inbox] [--check]` | All ledger records, only those needing review, or evaluate constraint checks (non-zero exit on a violated `must`, for CI) |
| `comindly erase <id…> [--yes]` | Erase ledger records (and relations about them) for good, including old snapshot versions; dry run without `--yes` |
| `comindly repos [add <folders or clone URLs…> \| remove <names…> \| scan <dir>]` | The engine's repositories, the list `comindly link` indexes when given no folders |
| `comindly team [set KEY=VALUE… \| key [--stdin]]` | The index's shared settings (models, LLM steps and limits, prices, format, SCIP, automatic re-indexing); `key` stores your own API key |
| `comindly prompts [show \| save \| activate \| deactivate \| delete]` | The LLM prompts indexing uses: their versions, a draft from a file, and which one is in use ([Configuration](../guides/configuration.md#models-and-prompts)) |
| `comindly config [show \| set KEY=VALUE…]` | The engine's settings: `index_dir`, `author_name`, `record_names`, `cache_ttl_secs` ([Configuration](../guides/configuration.md)) |
| `comindly app` | The current profile's web app: its own window on macOS, the browser on Linux and Windows |
| `comindly serve` | MCP over stdio for agents, forwarded to the current profile's `/mcp` |
| `comindly hook <agent> <event>` / `comindly init-hooks <claude\|cursor\|gemini\|codex\|generic>` | Agent hook adapter / config snippet |
| `comindly init-agents [--skill]` | When to use comindly's tools, for a repository's AGENTS.md or a Claude Code skill |
| `comindly doctor` | Check this machine's setup: the profile, the local engine (installed, running, same version), the sign-in, the index, whether an agent has comindly's tools, and Claude Code hooks |

## Against a server

With a server's profile in use, every command asks that server, with your role there: writes are
recorded as you. `preflight` reads the changes from git on this machine and asks the server what they
touch. `link` starts indexing on the server and shows its progress here; Ctrl-C cancels it there. A
server indexes its own repository list, so `link` takes no folders and `index` is refused there. `app`
opens the server's web app, where you sign in.

## The engine

`comindly-engine` is the engine itself: the index, the ledger, search, the web app, `/api/v1`, `/mcp`
and indexing jobs. You rarely start it by hand: `comindly` starts the local one when needed.

| Command | Purpose |
|---|---|
| `comindly-engine [--index-dir <dir>] [--host <addr>] [--port <port>]` | Without `COMINDLY_PUBLIC_URL`: this machine's engine on 127.0.0.1 (port 7879, or a free one), no sign-in, a local token. With it: a team server on 0.0.0.0:7878 with sign-in and roles ([Self-hosting](../guides/self-hosting.md)) |
| `comindly-engine window <url>` | The app in its own window, for the engine at `url` (macOS; `comindly app` uses it) |
| `comindly-engine eval [fixture] [--scip <dir>] [-v]` | Graph quality on a gold fixture (precision/recall per edge kind) |

`--index-dir` is a folder or a bucket location (`s3://`, `gs://`, `az://`) and defaults to the
configured index location (`comindly config set index_dir=…`).

## Flags

Run `comindly <command> --help` for every flag.
