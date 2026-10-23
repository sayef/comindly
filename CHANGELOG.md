# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Roles on a server: admins, members and viewers. Viewers read everything; members also add to the
  ledger and conventions; admins also manage people, repositories, settings, secrets and indexing. An
  agent works with the role of the person whose token it uses.
- `comindly login <url>`: sign the command line in to a server by approving it in the browser (a
  `/device` page shows the machine and a code to compare); `comindly logout` revokes its token on the
  server and forgets it. `find`, `search`, `explore`, `why`, `ledger`, `flow`, `stats`, `contracts`,
  `packages`, `conventions`, `preflight`, `remember`, `review` and `link` then ask the server and print
  what a local run prints (`preflight` reads the changes from git here); `link` starts indexing there and follows it, and Ctrl-C
  cancels it there. `--server <url>`, `COMINDLY_SERVER` and `COMINDLY_TOKEN` (for CI) pick a server for
  one command; `--local` uses the index on this machine.
- `comindly ui` against a server: the terminal UI shows the server's index (`GET /api/v1/view`,
  gzip-compressed), sends reviews to the server with your role, and fetches it again with `r`, which
  also re-reads a local index.
- `comindly app` against a server: the window opens the server's web app, with the server in its
  title; you sign in in the window, and **View → Open Link from Clipboard** signs it in with an
  emailed link.
- Indexing jobs on a server keep a record: who started them, their status and their events, also after a
  restart. `/api/v1/jobs` lists them and `/api/v1/jobs/{id}/events` streams a running job's events.
  A job cut off by a crash shows as interrupted.
- Indexing jobs queue: starting one while another runs (from the app, `comindly link` or the automatic
  re-index) waits its turn instead of being refused. Up to 5 wait; asking again for the same indexing
  returns the waiting job. The app and `comindly link` show "Queued" with the position and can cancel
  it; queued jobs left by a restart are marked interrupted.
- `comindly server` runs a team server (the app, `/api/v1` and `/mcp`, listening on `0.0.0.0:7878`);
  it refuses to start without `COMINDLY_PUBLIC_URL`. The container image runs it.
- `comindly doctor` shows which server the command line is signed in to, as whom, and whether its
  token still works.
- `GET /ready` for orchestrators: `200` once an index is loaded and the accounts can be read.
- `comindly config` shows your preferences; `set`, `import <file> [--override]`, `export [file]` and
  `history` change them, read and write them as TOML, and list earlier versions.
- The index's shared settings also hold `scip` and `auto_index`.
- Personal access tokens expire: 30 days, 90 days (the default), a year or never, chosen when one is
  created. Expired tokens stop working, show as expired and are removed 30 days later. A command line's
  token from `comindly login` lasts 90 days; when it expires, the command says to sign in again.
- An audit log for admins: Settings → Audit and `GET /api/v1/audit`, newest first, with paging and
  filters by who and what. Sign-outs are recorded too; token values and codes never are.
- comindly is also published as `comindly-core` (the engine, including indexing as an async,
  cancellable pipeline that reports events), `comindly-server` and `comindly-ui`.

### Changed

- `comindly ui --web` is the web UI on this machine; a team server runs with `comindly server`.
- `preflight` treats a constraint whose check passes as verified, as `ledger` does.
- The HTTP API moved to `/api/v1`, and every error has one shape, `{"error": {"code", "message"}}`, with
  a matching status.
- Your preferences are kept in a versioned table instead of `config.toml`. An existing
  `config.toml` is imported on the first run and renamed `config.toml.imported`; a later one is not read.
  Models, LLM steps, limits, the output format, SCIP and automatic re-indexing are the index's shared
  settings only, and settings from an old `config.toml` move there.
- A server keeps no configuration file: the repositories it indexes are kept with its accounts, and
  settings a 0.3 server kept move over on its first start.
- A server reads and checks its settings when it starts and stops with a message when one is missing or
  wrong. On SIGTERM it stops taking requests and lets a running job stop at its next checkpoint.
- Indexing from the app or the server runs inside the process as a job; cancelling stops it at its next
  checkpoint, LLM requests in flight included.
- Admins named in `COMINDLY_ADMIN_EMAILS` cannot be demoted in the app.

### Removed

- `comindly config init`; `comindly config export` writes a file to start from.

## [0.3.4](https://github.com/sayef/comindly/compare/v0.3.3...v0.3.4) - 2026-10-23

### Fixed

- Cancel stops indexing in container images too; it relied on a `kill` program that slim images lack.

## [0.3.3](https://github.com/sayef/comindly/compare/v0.3.2...v0.3.3) - 2026-10-23

### Added

- Team settings for what LLM steps cost: most symbols to summarise, most flows to walk through, LLM
  requests at once and the model's prices; search embeddings on or off; and whether agents get
  markdown or JSON text. In the app and in `comindly team set`.
- `COMINDLY_LOG_FORMAT=json` (set in the container image): every message as one JSON event with
  timestamp, level, step and fields, including progress, tokens and cost, for Datadog, CloudWatch or Loki.
- The app shows indexing as steps with results and warnings, with a live progress bar and its tokens
  and cost; the header shows a short status.
- **Add all** for repositories the index has but this server has no folder for.

### Changed

- Summaries, flow walkthroughs and style guides are reused while their input is unchanged and saved as
  they are written, so an interrupted indexing run continues where it stopped instead of starting over.
- A limit always picks the same symbols (ties are broken by id).
- Team settings saved on a server record the admin who saved them; the separate LLM card is gone there.

## [0.3.2](https://github.com/sayef/comindly/compare/v0.3.1...v0.3.2) - 2026-10-22

### Fixed

- A server keeps its settings and repository list with its accounts at the index location and restores
  them when a new container starts; until now a redeploy forgot which repositories it indexes.

## [0.3.1](https://github.com/sayef/comindly/compare/v0.3.0...v0.3.1) - 2026-10-22

### Fixed

- On a server, indexing started from the app uses the LLM key and git tokens saved under Integrations;
  0.3.0 skipped summaries, flow walkthroughs and style guides when the key was only saved there.
- The indexing log also goes to the server's output, so platform logs (Datadog, CloudWatch) show it.

## [0.3.0](https://github.com/sayef/comindly/compare/v0.2.0...v0.3.0) - 2026-10-22

### Added

- `GET /health` answers without signing in, for load balancer and orchestrator health checks.
- Settings → Members shows when admins named by the deployment were last seen.

### Changed

- Agents that connect to a server before its first index get `503` with the reason instead of `500`.
- A server keeps its members, sessions, tokens and stored secrets in a LanceDB table at the index
  location instead of a SQLite file, so containers need no volume; every record is signed with
  `COMINDLY_SECRET_KEY`, which server mode now requires. Members of a 0.2.0 server are invited again.

## [0.2.0](https://github.com/sayef/comindly/compare/v0.1.0...v0.2.0) - 2026-10-21

### Added

- Server mode for teams (`COMINDLY_PUBLIC_URL`): sign-in entirely by email or entirely by codes
  admins hand out (`COMINDLY_SIGN_IN`), invitations tied to one address, admins and members, personal access tokens for agents, and the server's GitHub, GitLab
  and LLM tokens kept encrypted or taken from the deployment's secrets (`*_FILE` too).
- Agents connect to a server over HTTP at `/mcp` with a personal access token; Settings → Your access
  shows the setup for Claude Code, Cursor, Codex and VS Code.
- Each release publishes a container image for amd64 and arm64 to `ghcr.io/sayef/comindly`.
- `comindly doctor` points out `aws_profile` and `sso_auto_login` left in `config.toml` from 0.1.0.
- A container image (`Dockerfile`) and a local stack (`compose.yaml`, `scripts/docker-local.sh`) with
  an S3-compatible store and a mail catcher.
- Settings → Repositories → **From GitHub or GitLab**: list an organisation or group (github.com, GitHub
  Enterprise, gitlab.com, self-hosted GitLab with subgroups), pick repositories, and comindly clones and
  keeps them itself; `comindly repos add <clone URL>` does the same from the terminal.
- A team's repository list in its settings: colleagues see the repositories they are missing and add them
  in one click.
- **Create a team** on the first screen, next to **Join a team**; joining works before the first index.
- **Flows**: every flow from its entry point across HTTP routes, topics and queues into the services
  that handle them, listed with the repositories it passes through and opened as a tree of steps;
  `/api/flows`. Indexing narrates the flows that reach furthest, across repositories first.
- comindly's icon in the macOS Dock and app switcher for `comindly app`.
- Activity in the app's header (indexing, any action in progress) with **Cancel**, and toasts
  when actions finish; indexing stops with everything it started, and a cancelled request
  stops its work on the server.
- `install.sh` verifies the download's SHA-256, and every release is installed on Linux and macOS and with
  `cargo install` before it counts.

### Changed

- The org map as a tree from the entry points (or any repository) to what each uses, or what uses it,
  opened box by box with the contract on each branch; cycles between repositories are found, looped back
  in amber and listed.
- A new look for the app: dark by default with a light variant, Geist and Geist Mono bundled with the
  app, an Overview that puts the org map next to what needs attention and the flows that reach
  furthest, and the index version and its age in the header.
- Docs: ways to run comindly on Getting started, deployment with Docker and Kubernetes under
  Self-hosting, and a guide to using the crate as a library.
- Indexing always uses each repository's current default branch, asked from the remote; the per-repository
  branch setting and `repos add --branch` are gone.
- Conventions are one filterable, sortable table; a row expands to the code that follows or deviates
  from it. Org-wide examples name their repository.
- Connect is now **Get started**, and Repositories moved under **Settings**. The separate Clone dialog is
  gone: **From GitHub or GitLab** takes an organisation, a group or a single repository.
- Messages read as sentences, long commands wrap, and storage errors are one line.
- An index can live in Google Cloud Storage (`gs://`) and Azure Blob Storage (`az://`) as well as S3 and
  folders, read with the credentials the machine or container already has.

### Removed

- AWS profile handling: the profile setting, `join --profile`, the app's AWS sign-in and
  `sso_auto_login`. Storage access comes from the platform's environment.

## [0.1.0](https://github.com/sayef/comindly/releases/tag/v0.1.0) - 2026-10-19

First release of comindly: cross-repo code intelligence for coding agents, in one binary and your own
storage.

### Added

- Indexing of many repositories into one versioned graph in a local folder or S3 (`comindly index`,
  `comindly link`), with tree-sitter for Python, TypeScript/JavaScript, Go, Java, Kotlin and Rust,
  SCIP indexes when present, and incremental rebuilds.
- An MCP server (`comindly serve`) and CLI for agents and people: hybrid code search, zoom, blast
  radius, context packs, flows and evaluation (`comindly eval graph`).
- Service-contract graph: HTTP routes and clients (FastAPI, Next.js, Express, Spring MVC, Feign,
  RestTemplate, WebClient, Go net/http/gin/echo/chi, axum, actix, reqwest, httpx/requests/fetch/axios),
  SQS/SNS/S3 (also through wrappers and constructors), SQL tables, Terraform queues/topics/buckets and
  subscriptions, values from YAML config; `comindly contracts` and the MCP `contracts` tool.
- Package versions and version skew across repos (`comindly packages`).
- Conventions and co-change recipes (`comindly conventions`, MCP `guide`); the team can add, edit and hide
  conventions from the CLI or the web UI.
- Reasoning ledger: typed records anchored to symbols, stored in your own store, with derived staleness,
  trust levels, review (`remember`, `why`, `review`, `ledger`), self-enforcing constraint checks, SSH
  signing, compaction and erasure. Records come from people and from agents through MCP and hooks, show
  who wrote and reviewed them, and can be confirmed, archived, invalidated or deleted.
- `comindly preflight` for uncommitted changes and MR pipelines; Claude Code and generic agent hooks with a
  local mirror for remote indexes; `comindly doctor`.
- A desktop app on macOS (`comindly app`: the web UI in a native WebKit window), the local web UI
  (`comindly ui --web [--open]`, guarded by an access token) and the terminal UI (`comindly ui`): org and
  symbol dependency graphs, a version slider, contracts, packages, ledger review and a command palette.
- Repositories and Settings in the app: what is in the index and where it lives on this machine, scan,
  clone over HTTPS or SSH, index with progress in comindly's own worktrees; `comindly repos`.
- Team settings stored with the index (LLM, embedding model, LLM steps), which win over personal ones;
  API keys stay per person in `~/.config/comindly/credentials` or `OPENAI_API_KEY`; `comindly team`,
  `comindly join` and a Join flow in the app.
- S3 through the standard AWS chain (profiles, SSO, `credential_process`, custom endpoints such as MinIO),
  with S3 errors explained and credentials kept out of messages.
- Docs at comindly.pages.dev, and `examples/acme`, a small example shop of five repositories used for the
  screenshots (`scripts/screenshots.sh`).
