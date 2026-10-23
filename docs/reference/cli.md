# Command line

| Command | Purpose |
|---|---|
| `comindly index <repo> [--enrich] [--flows] [--guide] [--no-embed] [--incremental]` | Index a single repo (embeds by default) |
| `comindly link <repos…> [--enrich] [--flows] [--guide] [--no-embed] [--incremental]` | Link several repos (cross-repo edges + blast radius) |
| `comindly explore <focus> [--index-dir <dir>]` | Zoom, blast radius, and context pack for a symbol |
| `comindly search <query…> [--repo <name>] [--index-dir <dir>] [--format md\|table]` | Graph-aware hybrid code search |
| `comindly find <query> [--repo <name>…] [--index-dir <dir>]` | Locate symbols by name/path substring |
| `comindly repos [--index-dir <dir>]` | List indexed repositories + symbol counts |
| `comindly stats [--repo <name>…] [--index-dir <dir>]` | Per-repo stats: symbols, edges, kinds, enrichment coverage |
| `comindly guide [--repo <name>…] [-o <file>] [--index-dir <dir>]` | Per-repo inferred coding style guides (built with `--guide`) |
| `comindly flow <focus> [--index-dir <dir>]` | Pre-generated flow walkthrough + live call trace |
| `comindly changed <repo> [--since <sha>]` | Files changed since a commit (git diff) |
| `comindly serve [--index-dir <dir>] [--format md\|json]` | MCP server over stdio |
| `comindly contracts [--type http\|sqs\|sns\|s3\|table] [--dangling]` | Service contracts with providers and consumers (FastAPI, Next.js, Express, Spring MVC, Feign, RestTemplate, WebClient, Go net/http/gin/echo/chi, axum, actix, reqwest, httpx/requests/fetch/axios, boto3, SQL; Terraform queues/topics/buckets and subscriptions) |
| `comindly packages [--skew]` | Internal packages: publisher, consumers, version skew |
| `comindly conventions [--repo <name>]` | Measured conventions, org baseline, recipes from history, and the team's own (each with an id) |
| `comindly conventions add "<rule>" [--repo <name>] [--dimension <d>] \| edit <id> "<rule>" \| rm <id> \| restore <id>` | Add a team convention, rewrite or hide one (measured ones can be restored); stored under `<index>/_conventions/`, untouched by re-indexing |
| `comindly preflight [repo] [--base <rev>] [--strict]` | Before committing or in MR CI: constraints, dead ends, failing checks, contracts, blast radius, missing companions ([MR job](../guides/ci.md#merge-request-preflight)) |
| `comindly remember <kind> --on <symbol> --title …` | Record a decision, failed attempt, constraint, gotcha, intent or question |
| `comindly why <symbol>` | Reasoning blame for a symbol (status, trust, evidence) |
| `comindly review <id> <confirm\|archive\|invalidate\|supersede\|dispute> [--reason "…"]` | Act on a ledger record; `comindly erase <id>` deletes one |
| `comindly ledger [--inbox] [--check] [--compact]` | All ledger records, only those needing review, evaluate constraint checks (non-zero exit on a violated `must`, for CI), or fold the inbox into the Lance snapshot |
| `comindly erase <id…> \| --actor <id\|me> [--yes]` | Erase ledger records (and relations about them) for good, including old snapshot versions; dry run without `--yes` |
| `comindly doctor` | Check this machine's setup: index and ledger reachable, local mirror, hooks, LLM key, signing, SCIP indexers, and the server sign-in |
| `comindly hook <agent> <event>` / `comindly init-hooks <claude\|cursor\|gemini\|codex\|generic>` | Agent hook adapter / config snippet |
| `comindly eval graph [--scip <dir>]` | Graph quality on the gold fixture (precision/recall per edge kind) |
| `comindly repos [add <folders or clone URLs…> \| remove <names…> \| scan <dir>]` | Indexed repositories, and the list `comindly link` indexes when given no folders |
| `comindly app [--index-dir <dir>]` | The UI in its own window (macOS) |
| `comindly team [set k=v… \| key]` | The index's shared settings (models, LLM steps and limits, prices, format, SCIP, automatic re-indexing); `key` stores your own API key |
| `comindly join <index>` | Use a team's index on this machine |
| `comindly ui [--web [--port 7878] [--host <addr>] [--open]] [--index-dir <dir>]` | Terminal UI (incl. a ledger timeline), or the web UI on 127.0.0.1 (overview, contracts, flows, ledger review, explorer with a neighbourhood graph, conventions) |
| `comindly server [--port 7878] [--host 0.0.0.0] [--index-dir <dir>]` | Run a team server: the app, `/api/v1` and `/mcp` with sign-in; needs `COMINDLY_PUBLIC_URL` ([Self-hosting](../guides/self-hosting.md)) |
| `comindly cache refresh [--index-dir <dir>]` | Refresh the local mirror of a remote index that hooks read |
| `comindly config [set k=v… \| import <file> [--override] \| export [file] \| history \| path]` | Your preferences on this machine, kept with their history; TOML files in and out ([Configuration](../guides/configuration.md)) |
| `comindly login <url>` / `comindly logout` | Sign in to a comindly server by approving this command line in the browser, or sign out: revoke the token on the server and forget the sign-in |

## Against a server

After `comindly login <url>`, these commands ask the server instead of an index on this machine:
`find`, `search`, `explore`, `why`, `ledger`, `flow`, `stats`, `contracts`, `packages`,
`conventions` (listing and changes), `preflight`, `remember`, `review` and `link`. They print what the
same command prints locally; writes are recorded as you, with your role on the server. `preflight`
reads the changes from git on this machine and asks the server what they touch. `explore` with repo
folders parses them here. `link` starts indexing on the server and shows its progress here;
Ctrl-C cancels it there. The server indexes its own repository list, so `link` takes no folders and
`index` is refused. `ui` shows the server's index in the terminal (reviews go to the server, <kbd>r</kbd>
fetches it again), and `app` opens the server's web app in the window, where you sign in. `ui --web`
always serves this machine's index.

`--server <url>` (or `COMINDLY_SERVER`) picks a server for one command; `COMINDLY_TOKEN` stands in for
a sign-in, for CI. `--local`, or an explicit `--index-dir`, uses the index on this machine. Every other
command always runs locally.

## Flags

Run `comindly <command> --help` for every flag. `--index-dir` is a folder or a bucket location (`s3://`, `gs://`, `az://`) and
defaults to the configured index location.
