# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Changed

- Agents act on any record with their person's OK, including retiring one a person wrote; a `dispute` still waits for a look.
- Every record and review carries a reason: `remember` needs a body and `review` a reason, in the CLI, the app and MCP.
- Agents are asked to confirm records they relied on and checked; each person confirming counts once, and cards show "Confirmed by N others". A person's confirmation, in the app or through their agent with `from_person`, makes a record T3.
- Needs review holds only disputed, stale, orphaned and likely-repeat records; unconfirmed agent records no longer wait there.

## [0.5.0-beta.11](https://github.com/sayef/comindly/compare/v0.5.0-beta.10...v0.5.0-beta.11) - 2026-11-10

### Added

- An agent's `remember` is held back when a standing record on the same code reads alike: the reply names it, so the agent confirms that one, or writes its own with `distinct` saying how it differs (kept as "Differs from").
- A record that reads like an earlier one on the same code waits in Needs review as a possible repeat, with **Same as earlier** (archive it) and **Keep both** (confirm it); `why` shows it too.

### Fixed

- A record a person confirmed after its code changed shows where the code is now, not where it was written.

## [0.5.0-beta.10](https://github.com/sayef/comindly/compare/v0.5.0-beta.9...v0.5.0-beta.10) - 2026-11-10

### Added

- The Ledger page filters by status, kind and trust, kept in the link.

### Changed

- Repositories and Your access are pages of their own in the sidebar; Settings keeps what applies to everyone (General, Models & prompts, Members, Integrations) and gains an About section. The Teams card moves to the bottom of Repositories, and the audit trail becomes Activity → Changes. Old /settings/repositories, /settings/access and /settings/audit links redirect.

### Fixed

- Confirming a stale record clears it: a confirmation now vouches for the code as it is indexed, so the record turns active until that code changes again. Field names such as chosen_by read as "Chosen by".
- An agent's confirmation with a reason now counts as its person's (T3), as one made in the app does.

## [0.5.0-beta.9](https://github.com/sayef/comindly/compare/v0.5.0-beta.8...v0.5.0-beta.9) - 2026-11-10

### Changed

- The index button has one box per LLM step (summaries, walkthroughs, style guides), each starting
  from the team settings, instead of one box that turned them all on. `comindly index` and
  `comindly link` take `--steps summary,guide,flow`.

### Fixed

- On phones, choosing a run in Activity brings its details into view instead of leaving them below
  the list.
- On phones, the overview's Needs attention rows put the record's text on its own line instead of
  squeezing it between the badge and its kind.
- On phones, contract routes wrap instead of being cut short, so routes that share a start can be
  told apart.
- On phones, a ledger record's symbols sit under "About" and long names wrap instead of running past
  the card's edge.
- The remove buttons on team and symbol chips are easier to tap.
- Ledger records written by an agent through a gateway name the agent ("Claude Code") instead of
  the MCP library ("rmcp"); records already written that way read "an agent".
- Multi-line signatures are kept whole (a Python `def` whose parameters span lines), for search,
  the explorer, MCP answers and summaries. Summaries are written again on the next index.
- A summary with no stored key from an earlier index is written again instead of kept as it was.

## [0.5.0-beta.8](https://github.com/sayef/comindly/compare/v0.5.0-beta.7...v0.5.0-beta.8) - 2026-11-09

### Changed

- Rewritten built-in prompts for summaries and walkthroughs. A summary says what the code does, its
  effects (what it reads, writes, calls and publishes) and the contract it serves, in one sentence;
  its questions are phrased the way people search. A walkthrough follows one request or message from
  its trigger, across services, to its outcome. Saved prompt versions are flagged as based on an
  older built-in.
- Summaries see each symbol's code (up to 60 lines), docstring, class, callers, callees and
  contracts, not only its name, signature and path; walkthroughs see each step's summary and the
  entry point's code. Both send this to the LLM.
- A summary is written again when its code changes, not only when its name, signature or path does.
- The style guide prompt keeps at most 8 rules per section and leaves out what a formatter or linter
  already enforces.

### Fixed

- Tool-call log lines name the app behind a gateway call (`x-forwarded-client`, e.g. "Claude Code"),
  trusted only on requests carrying the gateway's secret, instead of the MCP library's own name.

## [0.5.0-beta.7](https://github.com/sayef/comindly/compare/v0.5.0-beta.6...v0.5.0-beta.7) - 2026-11-09

### Changed

- comindly is now two programs. `comindly` is a small command line that never opens an index itself:
  every command asks an engine's `/api/v1`. `comindly-engine` holds the index, the ledger, search,
  the web app, `/api/v1`, `/mcp` and indexing jobs.
- Which engine a command asks is a profile: `local`, the engine on this machine, or a team server.
  The local engine is started in the background when a command needs it, and installed beside
  `comindly` on first use when missing.
- `comindly login` and `comindly logout` act on the current profile; `login` takes no address. A
  sign-in from an earlier version becomes a profile called `default`. Profiles are kept in
  `~/.config/comindly/profiles.toml`, readable only by you.
- `comindly-engine` without `COMINDLY_PUBLIC_URL` serves this machine on 127.0.0.1 (port 7879, or a
  free one) with a local token; with it, it is the team server on 0.0.0.0:7878, as `comindly server`
  was. The container image's entrypoint is `comindly-engine`.
- `comindly serve` is MCP over stdio forwarded to the current profile's `/mcp`, so an agent set up
  with it works against the local engine and against a server.
- `comindly config` holds the engine's settings: `index_dir`, `author_name`, `record_names` and
  `cache_ttl_secs`. `comindly config set index_dir=s3://bucket/comindly` points the local engine at a
  bucket index.
- `comindly index` and `comindly link` with folders work on the local engine only and index the folders
  as they are; which steps run is up to the team settings, and `--no-llm` skips the LLM steps.
- `comindly app` opens the current profile's web app: its own window on macOS, the browser elsewhere.
- `install.sh` installs the command line; `COMINDLY_ENGINE=1` installs the engine as well, for offline
  machines and CI. Releases ship `comindly-<target>.tar.gz` and `comindly-engine-<target>.tar.gz`.
- Crates: `comindly-api` holds the wire types of `/api/v1` and a typed client (feature `client`);
  `comindly-server` is renamed `comindly-engine` and has the engine binary; `comindly` is the command
  line only and no longer a library. Library users depend on `comindly-engine`.

### Added

- `comindly use`: list the profiles, switch with `comindly use local` or `comindly use <name>`, and add a
  team server with `comindly use <url> <name>`, which also signs in. `--use <name>` on any command, or
  `COMINDLY_PROFILE`, picks a profile for one command or shell.
- `comindly engine start|stop|status|install` manages the local engine; `install` downloads
  `comindly-engine` of the same version from the GitHub release.
- The local engine serves `/mcp`, with its local token as the bearer.
- Agent hooks ask the current profile's engine, so they work signed in to a team server too. They never
  start the local engine and add nothing when no answer comes within 3 seconds.
- Every MCP tool call is logged (`kind: tool`) with the tool, the client's own name (as an agent or a
  review bot introduces itself), the signed-in email, the outcome and the time it took.
- Settings → General → Logging switches request lines and tool-call lines on and off at once, without
  a restart; a server keeps the choice. `COMINDLY_LOG_SKIP` (e.g. `access,tool`) sets where it starts.
- `GET /api/v1/answer/hook` and `GET /api/v1/answer/guide`; `POST /api/v1/index` takes `paths` on the
  local engine.

### Removed

- The terminal UI and `comindly ui`; `comindly app` opens the web app.
- `comindly server` (run `comindly-engine` with `COMINDLY_PUBLIC_URL`), `comindly join` (use
  `comindly use`, or `comindly config set index_dir=` for a shared index) and `comindly cache`.
- `--index-dir`, `--local` and `--server` on commands: use `comindly config set index_dir=` or `--use`.
- The per-step flags on `index` and `link` (`--embed`, `--no-embed`, `--enrich`, `--flows`, `--guide`,
  `--scip`, `--incremental`, `--since`): the team settings decide.
- `ledger --compact` (the engine folds new records in after each index run), `erase --actor`,
  `team key --default`, `config import|export|history|path` and `prompts save --sections`.
- `comindly eval graph` moved to `comindly-engine eval`.

### Fixed

- `comindly doctor` reports the profile, the local engine (installed, running, the same version), the
  sign-in, the index, whether an agent has comindly's tools and whether Claude Code hooks are set,
  instead of warning about an empty index on this machine when you use a server.
- A FastAPI router built from a subclass of `APIRouter` (`router = ScopedRouter(prefix=...)`) serves
  its routes, also when the subclass lives in another indexed repository. The subclass's name must end
  in `Router`.

## [0.5.0-beta.6](https://github.com/sayef/comindly/compare/v0.5.0-beta.5...v0.5.0-beta.6) - 2026-11-07

### Fixed

- Layout conventions for code at a repository's top level read "at the top level of the repository"
  instead of "in `./`", and org-wide ones name a real example directory.
- On phones, the prompt editor's Try row stacks instead of squeezing its field, and the API key
  heading wraps its badge onto its own line.

## [0.5.0-beta.5](https://github.com/sayef/comindly/compare/v0.5.0-beta.4...v0.5.0-beta.5) - 2026-11-07

### Added

- Settings → Models & prompts: the LLM setup and the prompts on one page, stored with the index.
  A model per task (`llm_model_summary`, `llm_model_guide`, `llm_model_flow`), for example a
  cheaper one for summaries and a stronger one for style guides.
- Prompts for summaries, style guides and walkthroughs. An edit is saved as a new version that is
  not used until someone activates it; deactivating goes back to the built-in prompt. A style guide
  version can also change what each section covers. A draft can be tried on one symbol, flow or
  style guide section next to what the index holds now. A model or prompt change applies to what
  the next index writes, partial or full, without redoing results already written. Prompt changes
  appear in the server's audit log, and the page says when a release changed a built-in prompt
  that an active version replaces.
- `comindly prompts`: list, show, save a draft from a file, activate, deactivate and delete.
- Rewrite everything: one index run that writes summaries, style guides or walkthroughs again for
  all code with the current models and prompts, instead of reusing what was written (Settings →
  Models & prompts, `comindly link --rewrite summary,guide,flow`, or `rewrite` on
  `POST /api/v1/index`).

### Changed

- Summaries, style guide sections and walkthroughs ask the LLM for structured output (a JSON
  schema) instead of a text format read back line by line. The system message also carries the
  schema and a filled-in example, so a model or server without structured output still replies in
  the right shape. Results already in an index stay valid.
- Conventions read as instructions ("Type-annotate parameters") with a word for how strictly the
  code follows them (always, usually, often or sometimes) instead of a percentage, and a note where
  there are few examples. The counts moved to the expanded row ("Followed by 412 of the 530
  functions with parameters"); org-wide rules name the repositories that share them. Agents get
  the same wording, with the strength, through MCP.
- Settings → General keeps the org name and how agents get text; everything about models moved to
  Models & prompts.

## [0.5.0-beta.4](https://github.com/sayef/comindly/compare/v0.5.0-beta.3...v0.5.0-beta.4) - 2026-11-06

### Added

- `COMINDLY_GATEWAY_URL`: with it, Settings → Your access shows members how to connect each agent
  through the gateway, signing in with the organisation's account instead of a token, including the
  Claude desktop app and claude.ai.

### Changed

- The org map tells shared data from dependencies: repositories that only share tables or buckets
  are joined by a dotted line without an arrow ("shares data with") and never count as a cycle,
  since a library reading a service's table would otherwise loop back. A cycle lists its loop step
  by step with the import or call each step goes through.

### Fixed

- `guide` with a `focus` returns only that repository's narrative guide (a dev index with 16
  repositories went from about 195,000 to 19,000 characters); without one, it names the
  repositories that have a guide.
- In Markdown mode (the default) tool replies carry only the Markdown: Claude Code showed the
  attached JSON instead, at twice the size. `format: json` still sends JSON with structured content.
- `contracts` lists a symbol on both sides of a contract once, with its id.
- `suggest` leaves out vendored code and puts the most depended-on code first.
- The release's install check runs on betas too; it was skipped with the Mac builds.
- Someone invited can connect their agent through the trusted gateway straight away; before, they
  had to sign in to the app once first.

## [0.5.0-beta.3](https://github.com/sayef/comindly/compare/v0.5.0-beta.2...v0.5.0-beta.3) - 2026-11-05

### Added

- Guidance for agents on when to use each comindly tool, in one place: the MCP server sends it when an
  agent connects, the session-start hook adds it when the repository is indexed,
  `comindly init-agents` prints it for a repository's `AGENTS.md` (`--skill` for a Claude Code
  skill), and the Get started page shows both to copy.

- **View as agent** on the overview, contracts, flows, conventions, repositories and every symbol,
  and on each contract, ledger record and repository: the answer the matching MCP tool gives an
  agent, to read or copy. It replaces the symbol page's "Copy context for an agent".

- `comindly search` and `find` take `--team` (repeatable or comma-separated; `none` for unlabelled
  repositories), locally and against a server.

### Changed

- Settings → Integrations: an admin can store a token or LLM key even when the deployment provides
  one; the stored one is used instead, and removing it goes back to the deployment's.
- Indexing status hangs under the header's right corner instead of floating at the bottom; the
  ledger's and conventions' add buttons moved into their filter rows.

### Fixed

- `comindly init-hooks cursor`, `gemini`, `codex` and `generic` print the hook commands to run, as
  documented, instead of refusing.
- "8 matches", not "8 matchs".

## [0.5.0-beta.2](https://github.com/sayef/comindly/compare/v0.5.0-beta.1...v0.5.0-beta.2) - 2026-11-05

### Added

- Teams: labels on repositories inside one org. A team picker in the header filters the org map,
  flows, contracts, ledger, packages, conventions and repositories; other teams' repositories
  fold into one node per team at the edge of the org map. Admins label repositories and manage teams;
  everyone picks their own teams. See [Teams](https://comindly.pages.dev/guides/teams/).
- An existing server gets one team on upgrade, named after the org, with every indexed repository and
  every member.
- Notes from other teams: ledger notes keep their writer's teams; a note about another team's code
  counts less until one of its members confirms it, and each team chooses whether such notes count
  right away, wait for a member, or are refused. The ledger lists the ones waiting for your teams.
- MCP: `search`, `find`, `repos`, `contracts` and `suggest` take a `team` filter; `repos` lists teams
  per repository and `ripple` counts the impact per team.

### Changed

- The app says "org" for the whole index and its people; "team" now means a team inside it.

- A deploy or restart pauses indexing instead of interrupting it: the job continues by itself once a
  server is back, under the same job and in its order, reusing what was already summarised.

## [0.5.0-beta.1](https://github.com/sayef/comindly/compare/v0.4.0...v0.5.0-beta.1) - 2026-11-04

### Added

- Sign-in through your identity provider (OpenID Connect): Microsoft Entra ID, Google Workspace, Okta,
  Keycloak, Cognito and others. Set `COMINDLY_OIDC_ISSUER`, `COMINDLY_OIDC_CLIENT_ID` and
  `COMINDLY_OIDC_CLIENT_SECRET`; members, invitations and roles stay in comindly, and
  `COMINDLY_OIDC_ALLOWED_DOMAINS` lets a company's people join as viewers.
- A server can offer several ways to sign in at once: `COMINDLY_SIGN_IN=oidc,email,code`.
- Agents through a gateway you run: with `COMINDLY_GATEWAY_SECRET`, a gateway that signs people in can
  name the member behind each `/mcp` call, so the server stays private and answers members only, each
  with their own role.

### Changed

- A new sign-in page: what comindly does, typed out line by line, beside every way the server offers to sign in.

## [0.4.0](https://github.com/sayef/comindly/compare/v0.4.0-beta.3...v0.4.0) - 2026-11-03

## [0.4.0-beta.3](https://github.com/sayef/comindly/compare/v0.4.0-beta.2...v0.4.0-beta.3) - 2026-10-27

### Added

- `auto_index_llm` runs summaries, style guides and walkthroughs on automatic re-indexes, for new or
  changed code only.
- Ledger records store the indexed commit of each anchor and up to three snippets of the code that
  shaped them (`remember --snippet path:START-END`, MCP `snippets`).
- `auto_index_minutes` sets how often automatic re-indexing checks the remotes (default 5), also on the
  Repositories page.
- The index stores the contents of the indexed files, once per distinct content, so code is shown
  from the index: on a server it no longer needs a checkout on disk, also right after a restart.
- Code shown on the Conventions page links to the file at the indexed commit on GitHub or GitLab.

### Changed

- `comindly server` logs to stdout.
- Re-indexing no longer re-embeds every service contract when nothing changed: changed files are
  looked up by repository, not by package.
- Recipe conventions show the commits they were learned from, linked to GitHub or GitLab, instead of
  a "Show code" that could not find them.
- The index records each repository's default branch, so the Repositories page shows it without a clone.
- A new theme for the app on its existing layout: cool blue-grey neutrals with one steel-blue accent,
  muted repository colours, and Atkinson Hyperlegible for text and code. Tokens
  and rules are in `ui/DESIGN.md`.
- The sidebar collapses to icons (button beside the logo, or `⌘ \`); on phones it opens as a menu.
- On phones, tables become two-line rows, filters fit the screen, and the org map picks its repository
  from a list so the graph shows first.
- Irreversible actions ask once more in place: revoking a token, removing a repository, member or stored
  secret, withdrawing an invitation, deleting a team convention and stopping an indexing run. Nobody
  can change their own role.
- Settings keep unsaved edits when the data refreshes and warn before the tab closes with changes.
- Labelled fields, announced errors, keyboard access to the org map's arrows, and no light flash when
  dark mode loads.
- Filters on Contracts, Conventions, Flows, Ledger, Packages and Activity are part of the address.
- Ledger cards badge only what needs attention; flows open with the crossings between repositories and
  fold steps that stay in one repository.
- Org-wide conventions read like "Tests use pytest: in 3 of 10 measured repos", counting only the repositories with
  measured conventions.
- The org map shows one repository at a time: what uses it above, what it uses below, with the list of
  repositories (by how many depend on each) to move between them. Hovering or tabbing to an arrow lists
  the contracts and code references behind it; cycles are amber, and a key explains the arrows. The previous tree is one click away.
- Overview's figures show a breakdown where there is one (symbols per repository, contracts by type);
  what needs attention moves above the org map when there is something to review.
- The ledger opens on the timeline when nothing needs review, and its view is part of the address.
- A summary is written again only when its symbol's name, signature or path changed, not whenever
  its file did.
- Style guides are rewritten per section, and only when that section's evidence drifted.

### Fixed

- A server that restarted without its clones no longer re-indexes everything when nothing moved.
- Style guides of repositories removed from the index are dropped.

## [0.4.0-beta.2](https://github.com/sayef/comindly/compare/v0.4.0-beta.1...v0.4.0-beta.2) - 2026-10-26

### Changed

- A mint accent on neutral greys replaces teal and lime in the web app, the docs and the logo.

### Fixed

- A server's memory no longer grows with each index job. Jobs run in a child process again, as in
  0.3, which gets its keys on stdin and stops when the server does; one LanceDB cache (320 MB at most,
  down from 7 GB per connection) and one embedding model are shared by every reader.
- A job killed for running out of memory says so instead of failing without a reason.
- The ledger's Needs review tab lists only the records that need review, matching its count.
- The ledger history reads in the past tense: Confirmed, Archived, Invalidated.
- Setup checks show commands as code, and the access page shows dates as YYYY-MM-DD.

## [0.4.0-beta.1](https://github.com/sayef/comindly/compare/v0.3.4...v0.4.0-beta.1) - 2026-10-25

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
- Every response carries an `X-Request-Id` (a safe one sent by the caller is kept), and every log line
  written while handling a request carries it as `request_id`. An indexing job records the request that
  started it.
- An access line per request: method, path, status, time, request id and the member's email, never
  tokens or cookies. Health checks log it at debug; `COMINDLY_LOG_LEVEL` sets the level.
- Traces over OTLP/HTTP when `OTEL_EXPORTER_OTLP_ENDPOINT` is set: a span per request and per indexing
  job; `OTEL_SERVICE_NAME` names the service (`comindly` by default). Built into the container image and
  the Linux release binaries; a build from source needs `--features otel`, and a build without it says so
  when the variable is set. An unreachable collector delays shutdown by at most 2 seconds.
- **Activity** in the app: what comindly works on in the background. Tiles for what runs, waits,
  succeeded and failed filter the list, which groups runs by day; each run opens as a timeline of its
  steps with durations, results, warnings, errors, a live progress bar and LLM cost, with the plain log
  to copy. The header's indexing pill and the failure toast open the run there.
- `GET /api/v1/view` sends an `ETag` and answers `304 Not Modified` to a client that already has that
  version. `comindly ui` against a server keeps the last view in your cache folder (readable only by you,
  removed by `comindly logout`) and downloads it again only when it changed.
- `comindly app` on Linux and Windows opens the app in your browser, locally or on a server, instead of
  stopping with an error.
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
- Logs are written through `tracing`; readable and JSON lines keep their format and field names.
- A new look for the app and the docs site, with one palette: ink and paper with the mark's teal and
  lime, Geist throughout, a tinted pill for the current page, and the repository graph behind the
  sign-in, approval and first-run pages. The docs site has a new landing page and a **Copy page**
  button for every page's Markdown, and serves its fonts itself instead of loading them from Google.
- The indexing log in Settings → Repositories is the same step timeline as Activity.
- The terminal UI against a server knows your role: signed in as a viewer, it says read-only and
  explains instead of sending reviews the server would refuse.
- The tagline is "Your team's engineering memory for coding agents", in the README, the banners, the
  social preview and the crate description.

### Fixed

- Errors in the indexing log are shown in red; they used a colour the theme did not define.
- The social preview image and the app icon PNGs show the whole mark; ImageMagick's renderer dropped
  most of it, so the asset script uses resvg when it is installed.

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
