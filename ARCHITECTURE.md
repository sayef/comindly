# comindly — Rust Architecture

> Polyglot, cross-repo code-intelligence engine: an async server, an MCP endpoint for agents, and a CLI,
> in one static binary.
> Read-heavy / write-rare: the index is a **versioned artifact in your storage**, not a live shared DB.

## Decisions (locked)

| Question | Decision |
|---|---|
| Language scope | **Polyglot from day 1** via tree-sitter grammars. Infra langs low priority. |
| Rewrite | **Full Rust rewrite.** LLM enrichment stays, via Rig (provider-agnostic). |
| Cross-repo graph | **Core launch requirement** — federated symbol identity from day 1. |
| Embeddings | **Local model in CI** (Model2Vec). No source leaves org infra. |
| Storage | A folder or bucket (S3, Google Cloud Storage, Azure), read with the platform's own credentials. |

## The core reframe

The index only changes on push-to-master. So we do **not** run a concurrent
read/write database. Instead:

```
                    ┌──────────────────────────── CI on push to master ─────────────────────────┐
  repo A push ─────▶│ comindly index → Lance tables {symbols, edges, embeddings, chunks}          │
                    │              → write to  s3://comindly/<repo>/<commit>/                       │
                    │ link-resolver: merge stack-graphs across ALL repos → cross-repo edges       │
                    │              → s3://comindly/_graph/<ts>/  → atomically bump "latest" pointer  │
                    └───────────────────────────────────────────────────────────────────────────┘
                                                     │
   each dev's MCP server / CLI ──▶ watch "latest" ──▶ pull delta ──▶ build in-mem petgraph ──▶ serve locally
```

Lance gives atomic versioning + cheap deltas for free. Consumers are stateless
read-replicas. This dissolves DuckDB's single-writer pain and the shared-DB problem.

## Retrieval stance (research-informed — see landscape below)

The frontier is **de-emphasizing vector search for agents** (Anthropic removed it from
Claude Code; Cody replaced embeddings with SCIP). So our backbone is **deterministic**,
not fuzzy. Vectors are an *optional fallback* for NL "where is the code that does X",
never the primary path.

Retrieval modes, in priority order:
1. **Exact** — symbol/FTS lookup (deterministic, like grep/SCIP).
2. **Structural** — graph traversal: `ripple` (blast radius), `thread` (exec trace), `zoom`.
3. **Ranked** — personalized PageRank over the symbol graph (Aider-style) to pack the
   **minimal token-budgeted context**, seeded by the agent's current focus.
4. **Semantic** — vector similarity, *optional*, only when 1–3 miss.

## Storage split (do NOT put the graph in a vector DB)

| Concern | Store |
|---|---|
| Nodes + edges + FTS payload (the backbone) | **Lance/Parquet tables in S3, versioned** |
| Multi-hop traversal (`ripple`, `thread`, `zoom`) + PageRank ranking | **In-memory `petgraph`**, built from edge tables |
| Embeddings + code chunks (*optional* semantic fallback) | **Lance dataset in S3, versioned** |

Lance earns its place as the **versioned, S3-backed columnar artifact store** — that is
its real value here, independent of whether we lean on embeddings. The whole org symbol
graph (40+ repos) is low-millions of edges — fits in RAM; BFS blast-radius is microseconds.
No graph database needed. (KuzuDB is the fallback only if the in-mem graph stops fitting.)

## Federated symbol identity (the differentiator)

Global symbol ID follows the **SCIP** scheme so identity is stable and unique across repos:

```
<scheme> <package-manager> <package-name> <version> <descriptor>
e.g.  scip-python  pip  acme  1.4.0  `acme/foo`/bar().
```

- Each repo indexes locally → emits symbols + intra-repo edges + unresolved references.
- The **link-resolver** binds cross-repo references by SCIP id: an unresolved reference in
  `service-a` (`from acme.foo import bar`) binds to the *definition* symbol in
  `pkg-common`, producing a **cross-repo edge** (`cross_repo = true`).
- `ripple(pkg-common::X)` then answers *"who across the org breaks if I change this?"*
  — the query no per-repo tool gives you.

**Resolver strategy — tree-sitter-first, SCIP-optional:**
- **Default:** tree-sitter `tags.scm` extraction → fast, universal, *syntactic* symbol/ref
  graph for every language. Single binary, no per-language toolchain. Good enough for
  ranking + most navigation.
- **Precision upgrade:** ingest **SCIP indexes** where a team already emits them
  (`scip-typescript`, `scip-python`, `rust-analyzer`, `scip-java`) for *semantic* accuracy.
- **Note:** GitHub's `stack-graphs` (which I initially proposed) was **archived Sept 2025**.
  It remains usable as an optional resolver but is unmaintained — SCIP is the safe backbone.

## Layers

comindly is four crates in one cargo workspace. Each depends only on those below it, and the compiler
enforces the boundaries.

```
  comindly            the command: local mode, the web app's local server, and a client of a running
                      server (`comindly login`, `--server`). The terminal UI and the macOS window.
        │
  comindly-server     axum: `/api/v1`, MCP over HTTP and stdio, the web app, sign-in and roles, jobs,
                      the backends; and this machine's preferences, credentials and checks, since the
                      local app runs on it too.
        │         ╲
  comindly-core     comindly-ui
  the engine as an  the web app (React, in `ui/`), built and embedded
  async library
```

`comindly-core` reads no environment, no files of its own and no process-wide state, and prints
nothing: everything a call needs is passed in. The `comindly` library re-exports the server, which
re-exports core, so `comindly::graph` is `comindly_core::graph`.

### comindly-core

| Area | Modules |
|---|---|
| Model and parsing | `model`, `parse`, `lang/`, `facts`, `packages`, `manifests`, `infra`, `configvals`, `scip` |
| Binding and contracts | `resolve`, `resolve/contracts` |
| Derived knowledge | `conventions`, `recipes`, `checks`, `preflight`, `eval` |
| Index storage | `index` (Lance tables, versions, incremental change detection), `storage` (folder, `s3://`, `gs://`, `az://`) |
| Queries | `graph` (ripple, thread, zoom, flows, ranking), `embed` (Model2Vec + code-aware ranking), `search`, `answers` (what the read commands answer), `view` (what the apps show of an index) |
| LLM steps | `llm` (summaries, flow walkthroughs, style guides), `styleguide` |
| Ledger | `ledger`, `signing` |
| Sources | `worktree` (comindly's own checkouts, git credentials as a trait), `hosting` (listing GitHub organisations and GitLab groups), `git` |
| Indexing | `pipeline`: checkout, link and single-repo indexing as async, cancellable steps |
| Settings | `team` (an index's shared settings), `docstore` (a versioned settings document in LanceDB) |
| Plumbing | `context` (what a call needs), `events` (what it reports), `rt` (the process's one runtime) |

### comindly-server

| Area | Modules |
|---|---|
| HTTP | `web` and `web/*` (`/api/v1`, the embedded app, `/health`, `/ready`, graceful shutdown), `mcp` (the tools, over streamable HTTP and stdio) |
| Access | `server` (`ServerConfig`, roles, the default account store), `backends` (the traits and SMTP) |
| Work | `jobs` (runner, cancellation, records, live events) |
| This machine | `config` (preferences, identity and signing), `credentials` (API keys and the server sign-in), `doctor`, `cache` (the hooks' local mirror), `ui` (events as terminal lines or JSON logs) |

### comindly

`main` (commands), `remote` (commands answered by a server), `client` (the typed `/api/v1` client and
sign-in by browser approval), `tui`, `app` (macOS).

## Contexts, not globals

Core never asks "what is the current team" or reads a settings file. A call receives what it needs:

```rust
pub struct IndexContext {
    pub root: String,                     // where the index lives: a folder or a bucket location
    pub settings: Settings,               // models, LLM steps, limits, prices, SCIP: resolved once
    pub llm: Option<Arc<LlmClient>>,      // absent: LLM steps are skipped and say so
    pub git: Arc<dyn Credentials>,        // tokens for cloning and fetching over HTTPS
    pub report: Reporter,                 // events: steps, results, warnings, progress
    pub cancel: CancellationToken,
    pub cache: PathBuf,                   // comindly's checkouts of each repository
}
```

The server builds one per job from its settings and stores; the command line builds one from its
flags, the environment and the preferences. Ledger writes take the signer and the writer's identity as
arguments, and reads take the verifier. The web app holds the loaded index (`view::Data`) behind an
`Arc` and swaps it when a new version is indexed.

## Async model

- One Tokio multi-thread runtime per process (`rt::shared`). Core's API is `async`; no function builds
  its own runtime. Synchronous callers block through `rt::block_on`, which hands the worker's other
  tasks over while it waits.
- CPU-bound work (parsing, binding, graph building, embeddings) runs on the blocking pool
  (`spawn_blocking`), so requests stay responsive while a large index builds.
- I/O (object storage, LLM requests, git hosting APIs) is async. LLM requests share one client with a
  concurrency limit from the settings, and stop when the job is cancelled.
- Cloning, fetching and checkouts use the `git` program on the blocking pool: partial clones
  (`--filter=blob:none`) and the machine's credential helpers need it. Cancellation takes effect
  between repositories. `ssh-keygen` is used only for optional record signing.

## Jobs

Indexing runs inside the server as a job, not as a child process.

- **One at a time, in order.** A job has an id, a kind, who started it, a status (queued, running,
  succeeded, failed, cancelled, interrupted) and its events. Starting one while another runs queues it;
  up to 5 wait, and a request identical to a waiting one (same kind and options) returns that job
  instead of adding another. The running job never absorbs a request, since its input may already be
  stale. A queued job reads the repository list and settings when it starts, not when it was asked
  for. The automatic re-index queues the same way.
- **Cancellation.** A `CancellationToken` checked between units of work; cancelling stops LLM requests
  in flight and leaves everything already saved in place. Cancelling a queued job takes it out of the
  queue without starting it.
- **Resumable steps.** Summaries, flow walkthroughs and style guides are saved as they are written and
  reused while their input is unchanged, so a cancelled or interrupted job continues where it stopped.
- **Events.** Each job's events are kept with its record (the last 400; progress replaces earlier
  progress of the same step), streamed live (`/api/v1/jobs/{id}/events`, server-sent events) and
  written to the log. The record is saved when it is queued, when it starts, every 15 seconds and
  when it ends. Following a queued job streams a note with its position until it starts.
- **Restarts.** A job still marked running or queued at start-up is marked interrupted; a queued one
  says it never ran. Queued work is not run after a restart: what it would do is code, not data, and
  rebuilding it from a stale request could index what nobody asked for any more. On SIGTERM the
  waiting jobs are marked the same way.

The command line runs the same pipeline in the foreground and renders its events in the terminal; with
a server, `comindly link` starts or queues the server's job and follows its events instead.

## Backends

What the server stores and how it signs people in are traits, with comindly's defaults behind them:

| Trait | Default |
|---|---|
| `AccountStore` (members, roles, invitations, sessions, codes, personal access tokens) | signed LanceDB table at the index location (`_server`) |
| `AuditLog` (who changed what, for admins) | the same table |
| `Authenticator` (session cookie or bearer token → who) | the same table, plus the deployment's admin token |
| `DeviceSignIn` (command-line sign-in approved in the browser) | the same table; the issued token sealed until collected |
| `SecretStore` (git and LLM tokens) | the same table, values encrypted with `COMINDLY_SECRET_KEY` |
| `SettingsStore` (the repositories the server indexes) | the same table |
| `JobStore` (job records and their events) | the same table; the local app keeps them in memory |
| `Mailer` | SMTP (`COMINDLY_SMTP_URL`) |

Rows in the table are signed with a key derived from `COMINDLY_SECRET_KEY`, so rows written by anyone
else are ignored. Other implementations, for example a relational database or single sign-on, plug in
without changes to core or to the HTTP layer.

## Roles

Admin, member and viewer. Viewers read; members also write to the ledger and conventions; admins also
manage people, repositories, settings, secrets and indexing. The HTTP layer checks the role a request
needs, the account store checks who may change whom, and MCP tools that write check the role of the
person whose token the agent uses.

## API

- `/api/v1`: JSON. The web app and the command line use it. The answers the read commands print
  (`/api/v1/answer/*`) are the types in `comindly_core::answers`, computed by the same functions locally
  and on a server, so `--server` prints what a local run does.
- Errors: one shape, `{ "error": { "code", "message" } }`, with HTTP status codes that mean what they say.
- `/mcp`: MCP over streamable HTTP for agents, with a personal access token.
- `/health` (alive) and `/ready` (an index is loaded and the accounts can be read).

## Configuration

| Whose | Where | Read |
|---|---|---|
| The deployment's | `COMINDLY_*` variables, each also as `*_FILE` | once at start into `ServerConfig`, checked |
| A server's own | the `SettingsStore` | when used |
| An index's shared settings | `<index>/_team`, a versioned document | when an index is loaded, and after each save |
| One person's preferences | a versioned document in the config folder; TOML files are imported and exported | the app re-reads them every few seconds |

API keys and the server sign-in stay in a credentials file readable only by its owner.

## Observability

- Every message is an event: timestamp, level, kind, step, message and fields (counts, tokens, cost).
  `COMINDLY_LOG_FORMAT=json` writes one JSON object per line; otherwise readable lines.
- A job's events carry its `job_id`.
- Graceful shutdown: on SIGTERM or Ctrl-C, stop accepting requests, let a running job stop at its next
  checkpoint and save its record.

## What 0.4.0 changed

0.4.0 rebuilt comindly as described above, in one release: indexing moved into core as an async,
cancellable pipeline; one runtime per process; contexts instead of process-wide state; the stores,
sign-in and mail behind traits; indexing as jobs inside the server; `/api/v1` with one error shape;
roles; the command line as a client; settings moved to where they belong; and the crates above.
Deployments keep the same image, environment variables and storage layout; settings an older server
kept move over on first start, and nothing is re-indexed.

## Competitive landscape

Mid-2026 the field split three ways:

| Camp | Who | Strength | Weakness we exploit |
|---|---|---|---|
| Agentic grep / JIT | Claude Code, Cody (post-pivot) | Always fresh, simple | No cross-file/cross-repo structure, no blast radius |
| Semantic / vector | Cursor, Augment | Fuzzy recall | **Staleness**, noise; frontier de-emphasizing it |
| Deterministic graph | GitHub stack-graphs, Meta Glean, Sourcegraph SCIP | Precise, cross-repo | Heavy per-lang indexers, SaaS/enterprise, not agent-native |

Key signals: Anthropic removed vector search from Claude Code ("outperformed everything, by
a lot"); Cody replaced embeddings with SCIP; `stack-graphs` archived Sept 2025; Aider's
personalized-PageRank repomap sets the bar for token-efficient context; the winning metric
is **task-success-per-token** ("agents burn 10x the tokens they need").

**Our five differentiators:**
1. **Deterministic cross-repo graph** (SCIP identity) — precise like Glean, but self-hosted single binary.
2. **Zero-staleness team index** — push-to-master → versioned S3 → everyone shares one fresh index. Solves the problem that killed embeddings, at *team* scale.
3. **Cross-repo `ripple`** — org-wide blast radius; grep and per-repo tools can't answer it.
4. **Ranked, token-budgeted context packs over MCP** — Aider's insight, org-wide, delivered as *the minimal correct context to change safely*, not a search dump.
5. **Tree-sitter-first, SCIP-optional** — universal + fast by default, precise where it counts.

One-liner: *Deterministic, always-fresh, cross-repo code intelligence that hands your agent
the minimal correct context — self-hosted, single binary.*

Sources: [Sourcegraph — Agentic Coding in 2026](https://sourcegraph.com/blog/agentic-coding) ·
[AI agents don't need vector search anymore](https://buzzgrewal.medium.com/ai-agents-dont-need-vector-search-anymore-inside-the-agentic-search-stack-replacing-rag-in-2026-58efcabe4f6f) ·
[Code Intelligence Tools Compared](https://rywalker.com/research/code-intelligence-tools) ·
[Aider repomap](https://aider.chat/docs/repomap.html) ·
[SCIP announcement](https://sourcegraph.com/blog/announcing-scip) ·
[Stack graphs paper](https://arxiv.org/pdf/2211.01224) ·
[Cross-repository code navigation](https://sourcegraph.com/blog/cross-repository-code-navigation) ·
[AI assistants don't understand your code: LSP/SCIP](https://machinesdoitbetter.ai/ai-coding-assistants-dont-understand-your-code-lsp-scip-and-real-code-intelligence-2/)
