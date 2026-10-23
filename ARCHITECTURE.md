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

comindly is three layers in one repository. Each depends only on the one below it.

```
  comindly (CLI)      clap facade: local single-user mode, `comindly server`, and a client of a running
                      server (`--server <url>`, `comindly login`). Terminal rendering, the TUI, the desktop app.
        │
  comindly-server     axum: the versioned HTTP API, MCP over HTTP, the web app, sign-in, the job runner.
                      Builds a context per request and per job and hands it to core.
        │
  comindly-core       the engine as an async library: parsing, binding, contracts, flows, the graph,
                      search, LLM steps, the ledger, index storage. No environment, no config files,
                      no process-wide state, no terminal output: everything it needs is passed in.
```

The layers are crates in one cargo workspace, so the compiler enforces the boundaries.

### comindly-core

| Area | Modules |
|---|---|
| Model and parsing | `model`, `parse`, `lang/`, `facts`, `packages`, `manifests`, `infra`, `configvals`, `scip` |
| Binding and contracts | `resolve`, `resolve/contracts` |
| Derived knowledge | `conventions`, `recipes`, `checks`, `preflight`, `eval` |
| Index storage | `index` (Lance tables, versions, incremental change detection), `storage` (folder, `s3://`, `gs://`, `az://`) |
| Queries | `graph` (ripple, thread, zoom, flows, ranking), `embed` (Model2Vec + code-aware ranking), `search` |
| LLM steps | `llm` (summaries, flow walkthroughs, style guides), `styleguide` |
| Ledger | `ledger`, `signing` |
| Sources | `worktree` (comindly's own checkouts), `hosting` (listing GitHub organisations and GitLab groups), `git` |
| Indexing pipeline | `pipeline`: the steps from checkout to style guides as one async `Indexer` |
| Context and events | `context` (what a call needs), `events` (what it reports) |

### comindly-server

| Area | Modules |
|---|---|
| HTTP | `api` (`/api/v1`), `web` (the embedded React app), `mcp_http` (MCP over streamable HTTP) |
| Access | `auth` (sessions, personal access tokens, sign-in by email or code), `backends` (the default stores) |
| Work | `jobs` (queue, runner, cancellation, events, history) |
| Process | `config` (`ServerConfig`), `health`, graceful shutdown |

### comindly (CLI)

`main`, `commands/` (one module per command group), `client` (the typed `/api/v1` client), `render`
(terminal output of core events: colours, progress bars, tables), `tui`, `app` (desktop window),
`doctor`, `credentials`.

## Contexts, not globals

Core never asks "what is the current team" or reads `config.toml`. A call receives what it needs:

```rust
pub struct IndexContext {
    pub storage: Storage,                 // where the index lives, with its object store
    pub settings: Settings,               // models, LLM steps, limits, prices: resolved once
    pub llm: Option<Arc<dyn LlmClient>>,  // absent: LLM steps are skipped and say so
    pub embedder: Arc<Embedder>,
    pub secrets: Arc<dyn SecretStore>,    // git and LLM tokens, by name
    pub events: Arc<dyn EventSink>,       // progress, results, warnings
    pub cancel: CancellationToken,
}
```

The server builds one per request or job from its `ServerConfig` and its stores; the CLI builds one from
the command line, the environment and `config.toml`. Reads share an `IndexReader`: the loaded graph,
search index and ledger behind an `Arc`, swapped atomically when a new index version appears.

## Async model

- One Tokio multi-thread runtime per process, created in `main`. Core's API is `async`; no function
  builds its own runtime, and nothing blocks inside async code.
- CPU-bound work (parsing, binding, graph building, embeddings) runs on a bounded blocking pool
  (`spawn_blocking`), so request handling stays responsive while a large index builds.
- I/O (object storage, LLM requests, git hosting APIs) is async. LLM requests share one client with a
  per-call concurrency limit from the settings.
- Git operations use git2 on the blocking pool instead of the `git` program, so cloning, fetching and
  checkouts need nothing installed. `ssh-keygen` remains only for optional record signing.

## Jobs

Indexing runs inside the server as a job, not as a child process.

- **Queue and runner.** Jobs are queued and run with bounded concurrency; one indexing job per index at
  a time. A job has an id, a kind, who started it, a status (queued, running, succeeded, failed,
  cancelled, interrupted) and its events.
- **Cancellation.** A `CancellationToken` checked between units of work; cancelling stops LLM requests
  in flight and leaves everything already saved in place.
- **Resumable steps.** Summaries, flow walkthroughs and style guides are saved as they are written and
  reused while their input is unchanged, so a cancelled or interrupted job continues where it stopped.
- **Events.** Each job's events go to its `EventSink`: kept with the job, streamed to the app
  (`/api/v1/jobs/{id}/events`, server-sent events) and written to the log as JSON.
- **Restarts.** A job running when the process stops is marked interrupted at start-up and can be run
  again; nothing is lost that was already saved.

The CLI runs the same `Indexer` in the foreground and renders its events in the terminal.

## Backends

What the server stores and how it authenticates are traits, with comindly's defaults behind them:

| Trait | Default |
|---|---|
| `AccountStore` (members, sessions, codes, personal access tokens) | signed LanceDB table at the index location |
| `SecretStore` (git and LLM tokens) | the same table, values encrypted with `COMINDLY_SECRET_KEY` |
| `SettingsStore` (shared settings, repository list) | the same table |
| `JobStore` (job records and their events) | the same table, events capped per job |
| `Authenticator` (request → who, or refusal) | session cookie, personal access token, admin token |
| `Mailer` | SMTP (`COMINDLY_SMTP_URL`) |
| `EventSink` | the job's record, the app's event stream and JSON logs |

Other implementations, for example a relational database or single sign-on, plug in without changes to
core or to the HTTP layer.

## API

- `/api/v1`: JSON, with request and response types defined once in Rust and shared by the server and
  the CLI's client. The web app uses the same API.
- Errors: one shape, `{ "error": { "code", "message" } }`, with HTTP status codes that mean what they say.
- `/mcp`: MCP over streamable HTTP for agents, with a personal access token.
- `/health` (alive) and `/ready` (the index is loaded and the stores answer).

## Configuration

The server reads its configuration once at start into a `ServerConfig`: `COMINDLY_*` variables, each
also accepted as `*_FILE`, with defaults. Nothing below `main` reads the environment. Shared settings
changed in the app go through the `SettingsStore` and reach running code through a watch channel.

The CLI keeps `config.toml` for one person's preferences and credentials.

## Observability

- Every message is an event: timestamp, level, kind, step, message and fields (counts, tokens, cost).
  `COMINDLY_LOG_FORMAT=json` writes one JSON object per line; otherwise readable lines.
- Requests and jobs carry ids (`request_id`, `job_id`) through their events.
- Graceful shutdown: stop accepting requests, cancel running jobs at their next checkpoint, flush.

## Moving to this (0.4.0)

0.4.0 ships all of it at once. The order of work:

1. **Pipeline into core.** Move indexing (`cmd_link` and the `run_*` steps, now in the CLI binary) into
   `core::pipeline` as an async `Indexer` that reports events and honours cancellation.
2. **One runtime.** Replace the `*_blocking` wrappers (33 functions, each building a runtime) with async
   calls; CPU work moves to the blocking pool; git2 replaces the `git` program.
3. **Contexts.** Introduce `IndexContext` and `ServerConfig`; remove `team::current()`, the store
   singletons and `Config::load()` / environment reads below `main` (today in `llm`, `ledger`, `signing`,
   `embed`, `mcp`, `server`, `web`).
4. **Backends.** Put the stores, authentication and mail behind the traits above, the current behaviour
   as the defaults.
5. **Jobs.** The job runner replaces the child process and signals; events stream to the app.
6. **API.** `/api/v1` with shared types; the web app and a new CLI client use it; `--server` and
   `comindly login` in the CLI.
7. **Crates.** Split into `comindly-core`, `comindly-server` and `comindly`.

Deployments keep the same image, environment variables and storage layout; no re-indexing.

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
