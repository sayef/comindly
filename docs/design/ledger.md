# Comindly Ledger — design plan

> Status: **plan, not implemented**. Date: 2026-10-13. Sequencing: [roadmap.md](roadmap.md) (Track B).
> Decision 2026-10-16: no automatic sources. Git harvest and the MR distiller were removed; records
> come from people (`comindly remember`) and agents (MCP, hooks). Sections on harvest and the distiller
> are historical.
> Decision 2026-11-11: no stale status. Code changes add notes (moved, renamed, changed, with the commit
> the record saw); a record stays active until someone retires it. Sections on staleness are historical.
> One line: *the code graph says what breaks; the ledger says why it is this way, what was
> already tried, and what must not be done — anchored to symbols, across repos, never stale.*

---

## 1. Why this, why now

Agents now write most first drafts. They reason well inside a session and forget everything
after it. Teams re-try dead approaches, re-break invariants, and lose the "why" that used to live
in someone's head. Git records *what* changed and *who*; it buries *why*, and it never records
*what was tried and abandoned*.

Prior art (researched 2026-10, sources at the end) splits into four camps. Each solves a slice:

| Camp | Examples | Anchors to | Gap we exploit |
|---|---|---|---|
| Session capture in git | Entire, git-ai, Agent Trace, SpecStory | commit / line range | Raw transcripts, no distilled reasoning, no retrieval |
| Agent memory | Copilot memory, Claude memory, Graphiti/Zep, Mem0, Letta, claude-mem | text / file:line | Line anchors break; machine-local or SaaS; positive facts only |
| Review learnings | CodeRabbit, Greptile, Qodo rules | repo / file | Review-time only; not agent-facing at edit time |
| Decision records | ADR/MADR, Lore trailers, trace-mcp | path / doc | Path-level, age-based staleness, no cross-repo, no team sharing |

**What nobody does, and comindly can, because it already has a cross-repo SCIP symbol graph:**

1. **Symbol anchors that survive refactors and span repos.** Everyone else anchors to lines, paths
   or commits.
2. **Negative knowledge as a first-class type.** Failed attempts and rejected alternatives, with
   evidence. Almost every product stores only positive facts.
3. **Staleness driven by code change, cascading through the call graph.** Not age, not lazy LLM
   re-checks. The STALE benchmark (arXiv 2605.06527) shows models cannot be trusted to detect
   stale premises themselves; it must be structural.
4. **Proactive surfacing by graph impact.** "A constraint on a *callee* three repos away governs
   the edit you are about to make."
5. **Measured lift.** VibeMemBench (arXiv 2609.23570): 4 memory systems lost to *no memory* in
   11/12 pairings; verified, precise experience gained +1–4.5pp. The ledger ships with its own
   memory-on/off evaluation so we can prove it helps — and tune until it does.

Design consequence: **precision over recall, everywhere.** Injecting nothing beats injecting noise.

---

## 2. Principles (non-negotiable)

1. **Append-only, never rewrite.** Changes are new records (supersede / retract / confirm).
   LLM "merge" resolvers destroy data (Mem0); whole rewrites collapse detail (ACE).
2. **Anchored or it doesn't exist.** Every record cites ≥1 resolvable anchor (symbol, path,
   dependency, repo, org). No free-floating "memories".
3. **Evidence over assertion.** Attempts need evidence (revert SHA, failing test, error, benchmark).
   Decisions need alternatives. Trust is earned, not declared.
4. **Status is derived, not stored.** Active / stale / orphaned / superseded is computed from the
   record log + the current code index. Deterministic, recomputable, auditable.
5. **Your store, zero servers.** Records live in the user's own object store (S3 prefix or local
   path), next to the code index. Immutable record objects are the truth; Lance is derived.
6. **Facts about code, not about people.** No per-person analytics, ever. Pseudonymous actors.
   (See §12 — GDPR / Works Council.)
7. **Rendered as data, never as instructions.** Every injected record is quoted, attributed,
   factual, and tells the agent how to verify it.
8. **Fast enough to be invisible.** Hook paths < 100 ms. If it slows the agent down, it gets
   turned off.

---

## 3. Domain model

### 3.1 Record

```text
Record {
  id:          UUIDv7 (16 bytes; time-ordered)          // transaction time is embedded
  hash:        blake3(JCS-canonical JSON)                 // integrity + dedup + blob name
  schema:      1
  kind:        intent | decision | attempt | constraint | gotcha | question
  title:       ≤ 120 chars, factual
  body:        ≤ 2 KB markdown
  trigger:     ≤ 200 chars — "relevant when …" (drives retrieval; Devin-style)
  anchors:     [Anchor]   (≥ 1)
  evidence:    [Evidence]
  fields:      kind-specific (below)
  provenance:  { actor (pseudonymous), author_kind: human|agent|harvested,
                 agent?, model?, session_hash?, source: mcp|hook|git|mr|import|cli,
                 repo, branch, head_commit }
  lamport:     u64        // causal order across actors (git-bug style)
  valid_from:  commit     // code-time, not wall-clock
}
```

Kind-specific fields:

| Kind | Required | Optional |
|---|---|---|
| `intent` | need | link (ticket / incident / MR) |
| `decision` | chosen, alternatives[{option, why_not}] | reversibility, revisit_when |
| `attempt` | approach, outcome: failed\|reverted\|abandoned\|partial, evidence ≥1 | conditions (env, dep versions), retry_when |
| `constraint` | rule, severity: must\|should | check (compiled rule, §8), exceptions |
| `gotcha` | symptom, cause | workaround |
| `question` | question | owner_team, blocking |

### 3.2 Anchor

```text
Anchor {
  scope:      symbol | file | module | repo | org | dependency
  scip:       GlobalSymbolId?            // cross-repo stable identity
  repo, path, span?,
  commit:     where it was anchored
  sig_hash:   name + params + return
  body_hash:  normalized AST tokens (comments/whitespace stripped)
  shape_hash: body_hash with identifiers alpha-renamed (survives renames)
  dep?:       { package, version_range }  // e.g. lance "=1.0.1"
  role:       subject | context | evidence // subject drives staleness hardest
}
```

### 3.3 Relations (also records, also append-only)

`supersedes(old → new)`, `retracts(id, reason)`, `confirms(id, at_commit)`,
`disputes(id, reason)`, `relates(a, b, label)`. A `confirms` from reading the anchored code
(Copilot's validate-on-read) refreshes freshness.

### 3.4 Derived status (computed at compaction, never written)

```text
quarantined   trust T0
retracted     a retracts record exists
superseded    a supersedes record exists
orphaned      subject anchor's symbol no longer exists (kept in the Graveyard)
stale         subject body_hash drifted beyond threshold, or a dep/evidence anchor changed
dormant       not surfaced or confirmed for N days (still searchable; not auto-injected)
active        otherwise
```

### 3.5 Trust tiers

| Tier | Meaning | Auto-injected? |
|---|---|---|
| T0 quarantined | Mined from tool output, web content, unverified sources | Never |
| T1 asserted | Agent wrote it, no verified evidence | No (on request only, labelled) |
| T2 evidenced | Evidence verified mechanically (revert exists, test failed in CI, commit merged to default branch) | Yes |
| T3 confirmed | A human accepted it (inbox, MR, CLI) | Yes, ranked higher |
| T4 enforced | Has a compiled check that passes on the current code | Yes, and blocks in preflight/CI |

Promotion is automatic where mechanical (T1→T2 when the linked commit lands on the default branch).

### 3.6 Bitemporal, in code-time

- **Transaction time** = record id (UUIDv7) + ledger commit time. "What did we know at T?"
- **Valid time** = `valid_from` commit → first commit where status leaves `active`.
  "What was true about the code at commit C?" — enables `comindly why X --at <sha>`.
- Lance version numbers are local and not portable; time columns are explicit. Local Lance
  versions are tagged `ledger@<git-oid>` for reproducibility only.

---

## 4. Storage

Same model as the code index: the user points comindly at **their own** object store
(`--to s3://their-bucket/prefix` or a local path). The ledger lives under that prefix, next to the
graph. Nothing is pushed to git remotes, and nothing goes anywhere the user did not configure.

### 4.1 Source of truth: one immutable object per record

```text
<store>/_ledger/inbox/<uuid>-<blake3>.json     one object per record, written once
<store>/_ledger/tables/ledger_records         Lance snapshot: inbox key → record (derived, §4.3)
```

- Writers `PUT` with create-only semantics (`If-None-Match: *`, `object_store` `PutMode::Create`).
  Keys are unique, so writers never contend and never lock — the inbox is a **grow-only set CRDT**.
  Works the same on S3, GCS, Azure Blob and the local filesystem.
- Why not append straight into Lance: concurrent appends are safe (Lance commits optimistically and
  auto-rebases/retries compatible transactions), but one commit per record means many tiny
  fragments and constant compaction. Immutable objects + batched compaction keep Lance tidy.
- Least privilege falls out naturally: agents/devs need only `PutObject` on `_ledger/inbox/*`;
  only the compactor writes Lance tables; consumers only read.
- Optional per-record **SSHSIG** signature field (`ssh-key`, namespace `comindly-ledger`), reusing
  developers' existing SSH signing keys and `allowed_signers`.
- Git is used **read-only** for harvesting (reverts, Lore trailers, `Comindly-Record:` trailers);
  it is never the ledger store.

### 4.2 Compaction and freshness

- `comindly ledger --compact` (run by the CI index job, or by anyone) folds inbox objects missing
  from the snapshot into Lance via `merge_insert` on the inbox key, and deletes snapshot rows whose
  object is gone — idempotent, so a retried or duplicated compaction is harmless. Lance resolves
  concurrent commits itself, so CI and clients may both run it. (Implemented; the command was later
  removed, and `comindly erase` compacts.)
- Readers `LIST` the inbox (authoritative) and `GET` only keys missing from the snapshot, so a
  record is visible to the whole team seconds after it is written, and erasing its object
  removes it everywhere at once. No watermark is needed (harvested ids are UUIDv5, not ordered).
- One store per org already holds every repo's graph, so cross-repo and org-scope records need no
  special home — scope comes from the anchors.

### 4.3 Derived: Lance tables

| Table | Content |
|---|---|
| `ledger_records` | immutable rows, one per record (incl. relations) |
| `ledger_current` | derived status, trust, current re-anchored location, scores |
| `ledger_anchors` | exploded anchors → join key to `symbols` |
| `ledger_search` | FTS (BM25) + Model2Vec vectors over title/trigger/body; hybrid RRF like `search` |

Consumers pull the ledger with the graph in the same `serve --from <uri>` load.

### 4.4 Local layers

```text
.git/comindly/drafts/        distiller proposals awaiting review (local until accepted)
.git/comindly/transcripts/   optional, redacted, local-only pointers (hash → path); never shared
~/.cache/comindly/ledger/    mmap-able compacted view for the hook fast path
```

---

## 5. Capture — five channels, one schema

### A. Explicit (MCP / CLI)

- MCP `remember` — one tool, `kind` is a parameter; schema-validated, length-capped.
- CLI `comindly remember attempt --on <symbol> --approach … --outcome reverted --evidence <sha>`.
- Writes go to drafts or directly to the actor ref depending on trust config.

### B. Agent hooks (`comindly hook <agent> <event>`)

One adapter normalizes Claude Code / Codex / Cursor / Gemini CLI events:

| Normalized event | Claude Code | Action |
|---|---|---|
| session-start | `SessionStart` | Inject a ≤ 1.5 KB digest: active constraints + recent dead-ends for the cwd repo, progressive-disclosure index |
| pre-edit | `PreToolUse` (Edit/Write/MultiEdit) | **Just-in-time injection**: records anchored to symbols in the target file + 1-hop graph neighbors; dedupe per session |
| tool-failure | `PostToolUseFailure` | Buffer as candidate evidence (failing test, error) |
| revert-in-session | `PostToolUse` (git ops) | Candidate `attempt` |
| turn-end / compact | `Stop`, `PreCompact`, `SessionEnd` | Run the distiller (async) → drafts |

`comindly init-hooks claude|codex|cursor|gemini` writes the agent's hook config. Hooks prefer the
`mcp_tool` hook type where supported (no process spawn).

### C. Git harvest (deterministic, no LLM)

- **Reverts** → `attempt` (outcome `reverted`, evidence = revert SHA, T2 immediately).
- **Lore-protocol trailers** (`Constraint:`, `Rejected:`, `Directive:`, `Tested:` …, arXiv
  2603.15566) → typed records. We *ingest and emit* Lore — free interop.
- `Comindly-Record: <uuid>` trailer links commits ↔ records.

### D. MR / PR harvest

- MR descriptions and resolved review threads (GitLab / GitHub API) → distiller → drafts.
- Bot command in review: `/comindly remember constraint …`.

### E. Interop ingest (provenance only, never raw transcripts)

- Agent Trace records, git-ai notes, Entire checkpoint metadata, ADR/MADR directories.
- Used to enrich provenance ("this line came from session S") and to seed decision/intent drafts.

---

## 6. The distiller — the prompt that matters most

> Implemented in `src/distill.rs` (`comindly distill git`). **Decision 2026-10-15: comindly never reads
> agent session transcripts**; the transcript inputs, `.git/comindly/transcripts/` and the turn-end /
> compact hook below are not built. The distiller works from commit messages (and later MR threads).
> Ops: `new` and `supersede`; proposals land in the shared inbox at T1 (author `distilled`). A
> supersede or retract by a non-human author only retires machine records; a human-backed record
> is marked challenged and waits for a person.

Runs on turn-end/compaction, MR merge, or on demand. Provider-agnostic via Rig; works with local
models through any OpenAI-compatible endpoint.

**Input (pre-processed deterministically, not by the LLM):**
- Redacted transcript slice: system reminders and tool outputs stripped/truncated, secrets scanned.
- The session diff, with touched symbols resolved to SCIP ids and anchors.
- **Existing active records for those anchors** (so it emits deltas, not duplicates — ACE).
- Failure signals collected by hooks (failed commands, reverted hunks).

**Output:** JSON constrained by schema — a list of ops, max 3:
`new(record) | confirm(id) | supersede(id, record) | dispute(id, reason) | none`.

**Prompt contract (draft):**

```text
You extract durable engineering knowledge from a coding session. Default output is `none`.

Emit a record ONLY for:
  - an approach that was tried and failed, reverted or abandoned (kind=attempt)
  - an explicit choice between alternatives (kind=decision)
  - an invariant discovered the hard way (kind=constraint / gotcha)
Never emit: summaries of what was done, style preferences, facts obvious from the code,
anything about the people involved.

Each record must:
  - cite evidence by span id from <evidence> (a failing command, a revert, a test)
  - anchor to symbol ids from <touched_symbols> only
  - state a `trigger`: when would a future engineer need this?
  - be written as neutral facts, no imperatives

If an existing record in <known_records> already covers it: emit confirm(id) or
supersede(id) — never a near-duplicate.

Before answering, check each candidate: Would a senior engineer new to this code be
glad to be told this before editing it? If not, drop it.
```

**Post-validation (deterministic):** anchors resolve; evidence spans exist; secret scan; Unicode
tag/bidi stripping; imperative/instruction-phrasing lint; near-dup check (same anchors + vector
similarity); per-session rate limit. Survivors land in **drafts** at T1 (T2 if evidence verifies).

---

## 7. Retrieval and surfacing — the experience

### 7.1 Ranking

```text
score = anchor_proximity      // 1.0 on focus symbol, decays per graph hop (ripple/thread distance)
      × kind_weight           // constraint > attempt > gotcha > decision > intent (for edit tasks)
      × trust_weight          // T4 > T3 > T2; T1 only on explicit request
      × freshness             // active=1, stale=0.4 (shown with a warning), dormant=0.3
      × relevance             // hybrid BM25+vector of task text vs title/trigger/body
```

Hard caps: top-k with diversity (one per anchor cluster), token budget like `context_pack`.

### 7.2 Surfaces

| Surface | What the user/agent sees |
|---|---|
| `why <symbol>` (CLI + MCP) | Reasoning blame: intent → decisions → attempts → constraints, timeline, with status + trust badges |
| `context_pack` / `zoom` / `ripple` | Gain a "Known constraints & dead ends" section; `ripple` also lists *constraints affected downstream across repos* |
| **Just-in-time pre-edit hook** | ≤ 500 tokens, only when relevant, only T2+, once per session per record |
| `preflight` (CLI + MCP) | Input: plan text or diff. Output: conflicts with attempts/constraints on touched symbols + their ripple, with severity and evidence; runs compiled checks (§8) |
| MR bot / CI `comindly ledger check` | "This reintroduces approach X, reverted in !123 because Y" / "Violates constraint C (T4)" / "3 records anchored here are now stale — confirm or retract" |
| MCP resources | `comindly://why/{scip}` resource template; MCP prompt `why-is-this-like-this` |
| Revival notices | "Attempt failed under lance =1.0.1; you are now on 1.3 — retry condition met" |

### 7.3 Rendering format (what gets injected)

```text
<comindly-ledger source="comindly ledger" note="recorded facts; verify before relying">
[attempt · T2 · active] Redis cache for permission checks — reverted
  on: service-a `authz/check_permission().` (verified at 4f1c2a)
  evidence: revert 9b2e01 ("stale grants after role change"), MR !412
  relevant when: adding caching to authorization paths
  verify: read authz/check.py:40-88
</comindly-ledger>
```

Factual, attributed, bounded, with a verify pointer. Agents that confirm/retract after verifying
feed the freshness loop.

---

## 8. Constraints that enforce themselves

A constraint can optionally compile to a check, promoting it to T4:

| Check type | Example |
|---|---|
| **Graph rule** (native) | `forbid edge Calls from module:handlers/** to symbol:db.raw_query().` — evaluated on the comindly graph, cross-repo |
| **Dependency rule** | `forbid dep lance =1.0.1` |
| **ast-grep / Semgrep rule** | pattern-based, run if the tool is installed |
| **Test reference** | constraint is proven by test `tests/test_x.py::test_y` |

`preflight` and `comindly ledger check` evaluate them. A decision expressed as a rule cannot go
silently stale — it fails CI. The distiller may *propose* a rule; a human must accept it.

---

## 9. Staleness engine

Runs on every (incremental) index — the CI path we already have.

1. **Fingerprint** every symbol: `sig_hash`, `body_hash`, `shape_hash` (tree-sitter subtree,
   comments/whitespace dropped). Stored in the symbols table.
2. **Re-anchor** moved/renamed symbols: git `find_similar` for files → equal `body_hash`/`shape_hash`
   for symbols → fuzzy match (`imara-diff`) for the rest. Re-anchoring is derived (no new record).
3. **Classify drift** per subject anchor: unchanged → active; signature-only → active + note;
   body drift above threshold → **stale**; deleted → **orphaned**.
4. **Cascade**: evidence/context anchors changing → stale; constraint on a callee whose contract
   changed → dependents' records flagged (bounded hops).
5. **Dependency anchors**: lockfile moves outside `version_range` → attempts flagged as
   "retry condition met"; constraints flagged for re-check.
6. **Decay**: never surfaced/confirmed in N days (default 60) → dormant (not auto-injected).

Stale records surface in the review inbox and the MR bot; one keystroke to confirm/retract.

---

## 10. Interfaces

### 10.1 MCP (rmcp, upgrade to spec 2026-07-28)

New tools — deliberately few: `why`, `remember`, `preflight`, `review` (`confirm` / `retract` /
`supersede` / `dispute`). Existing `context_pack`, `zoom`, `ripple`, `search` gain ledger
sections. Resources + resource templates, one prompt, `outputSchema` + `structuredContent`
everywhere, resource subscriptions for ledger changes, human confirmation via the spec's
multi-round-trip `input_required` flow (replacing elicitation). No sampling (deprecated).

### 10.2 CLI

```text
comindly why <symbol|path[:line]> [--at <sha>] [--all]
comindly remember <kind> --on <anchor> …
comindly preflight [--diff [<range>] | --plan <file|->]
comindly ledger init|sync|compact|log|inbox|audit|check|import|export|gc
comindly hook <agent> <event>               # stdin JSON → stdout JSON
comindly init-hooks <claude|codex|cursor|gemini>
comindly app                                # was `comindly ui [--web]`; the terminal UI was removed
comindly eval ledger …                      # §13
```

### 10.3 Daemon fast path

`comindly serve` already exists. Hooks connect to it over a Unix socket when running; otherwise
they read the mmap'd compacted cache. Targets: hook p95 < 100 ms, `why` < 50 ms warm,
`preflight` < 300 ms, incremental compaction < 1 s for 1k new records.

---

## 11. UI — TUI and web, same backend

> Later: the terminal UI was removed. The web app is opened with `comindly app`.

### 11.1 `comindly ui` (TUI, ratatui 0.30)

Elm-style architecture (Model / Msg / update / view), one `tokio::mpsc` event bus, background
workers for index and ledger queries, pure rendering. gitui/bottom-style component structure.

| Screen | Content |
|---|---|
| **Home** | Fresh records, review inbox count, stale hotspots, "dead ends prevented this week" |
| **Explorer** | Symbol tree (left) · `zoom` neighborhood mini-graph (center) · `why` timeline (right) |
| **Timeline** | `git log` interleaved with reasoning lanes (decision / attempt / constraint), tig-like |
| **Inbox** | Drafts + stale records; `a` accept · `e` edit · `r` reject · `s` supersede · `c` confirm |
| **Preflight** | Paste a plan or pick a diff; conflicts with evidence, jump to anchors |
| **Graveyard** | Orphaned + reverted approaches, searchable — institutional memory of what failed |

Terminal graphs stay local (≤ a few dozen nodes, Sugiyama layout, image protocol on capable
terminals via `ratatui-image`, ASCII fallback). Big graphs go to the web UI.

### 11.2 `comindly ui --web` (embedded SPA)

axum + `rust-embed` serving a Vite/TypeScript SPA from the same binary on `127.0.0.1`, random
per-launch token, Host/Origin checks.

| View | Library |
|---|---|
| **Org map** with reasoning heat overlay (constraint density, stale ratio, dead-end clusters) | Sigma.js + graphology (WebGL) |
| **Ripple lens**: blast radius with every affected constraint lit up | Sigma.js |
| **Decision lineage**: supersede/dispute chains | @xyflow/react (small DAGs) |
| **Time slider**: the graph + ledger *as of* any commit (bitemporal) | d3 |
| **Record detail**: evidence, anchors (click → code), provenance, history | — |

Later: the same views as MCP Apps inside agent clients.

---

## 12. Trust, safety, privacy

- **Memory poisoning** (OWASP Agentic ASI06; MINJA, AgentPoison, MemoryGraft): trust tiers gate
  injection; T0 for anything derived from tool output/web; schema + length caps; Unicode
  tag/bidi stripping; instruction-phrasing lint; optional record signatures; `comindly ledger audit`
  lists all agent-authored records since T; retraction as a kill switch.
- **Secrets**: scanned before any record is written; raw transcripts never enter the shared store.
- **GDPR**: pseudonymous actor ids; no PII in record bodies (lint); configurable retention.
  Erasure is a real delete, unlike git history: remove the inbox object, `delete` the Lance rows,
  then clean up old Lance versions (deleted rows survive in prior versions until cleanup).
  Implemented as `comindly erase <id…> [--yes]` (an `--actor` form existed and was removed): deletes the inbox objects and
  relations about them, compacts, rewrites the snapshot and prunes all old versions, and leaves
  id-only tombstones (`_ledger/erased/<id>`) so `harvest` does not re-create harvested records.
  Run it while no other compaction is in progress (pruning skips the in-flight safety window).
  **Confirm the retention/erasure design with legal counsel before release** (this plan is
  not legal advice).
- **Works Council / EU AI Act**: the ledger describes code, not people. No per-person counts,
  rankings, or "who causes most failed attempts" views — enforced by not storing linkable
  identity in the shared ledger. Keeps it clear of employee-monitoring and high-risk use.

---

## 13. Proving it works — evaluation built in

`comindly eval ledger`:

1. **Replay benchmark from your own history.** Mine reverts and Lore `Rejected:` trailers from git
   history → tasks where a known-bad approach is tempting. Run the agent with ledger on vs off.
   Metrics: repeat-failure rate, task success, tokens, steps.
2. **Injection precision.** Of records injected, how many did the agent cite, verify, or act on?
   Track per kind and per tier; tune caps/weights until precision is high.
3. **Staleness accuracy.** Seed known drifts; measure stale/orphan detection precision/recall.
4. **Poisoning red-team.** Seed adversarial records (contradicting code, citing non-existent code,
   instruction-laden); measure injection rate and agent correction rate.
5. **Distiller quality.** Human-labelled sample: accept rate in inbox, duplicate rate.

Local counters only (no telemetry leaves the machine): "dead ends prevented" = preflight/JIT hits
followed by a plan change.

**Release gate:** ledger-on must beat ledger-off on the replay benchmark before auto-injection is
enabled by default.

---

## 14. Phases and exit criteria

| Phase | Scope | Exit criteria |
|---|---|---|
| **P0 Foundations** | rmcp 2.2 → 3.x (spec 2026-07-28); re-test lancedb 0.40 (lance recursion bug); symbol fingerprints in the index; `src/ledger/` module skeleton | Existing 10 MCP tools pass on new rmcp; fingerprints stable across reformatting |
| **P1 Ledger core** | Record schema, object-store inbox (create-only PUT), idempotent compaction to Lance, derived status, CLI `remember` / `why` / `ledger log|sync` | Many concurrent writers on S3 and local FS; compaction converges identically and is idempotent |
| **P2 Agent loop** | MCP `why`/`remember`/`review`; ledger sections in `context_pack`/`zoom`/`ripple`; Claude Code hooks (session-start digest, pre-edit JIT); **eval harness v0** | Dogfooded on comindly itself; JIT hook p95 < 100 ms; replay benchmark runs |
| **P3 Staleness engine** | Re-anchoring, drift classes, cascade, dependency anchors, decay, inbox | Seeded refactor suite: renames re-anchor, real changes flag stale, ≥ 95% precision |
| **P4 Harvest + distill** | Reverts, Lore trailers, MR import, distiller + drafts, Agent Trace / git-ai / Entire / ADR ingest; Codex/Cursor/Gemini adapters | Distiller inbox accept rate ≥ 60%, duplicate rate ≤ 5% |
| **P5 Preflight + enforcement** | `preflight`, compiled constraints (graph/dep rules, ast-grep), `ledger check` in CI, MR bot | A reverted approach re-proposed in a test MR is caught with evidence |
| **P6 TUI** | `comindly ui` screens of §11.1 | All review flows doable without leaving the terminal |
| **P7 Web UI** | `comindly ui --web` views of §11.2 | Org map renders 50k+ nodes smoothly; time slider works |
| **P8 Hardening** | Signing, poisoning red-team, GDPR erasure path, docs, public launch | Release gate of §13 passes |

P2 is the first moment of real value — keep P0–P2 tight and dogfood immediately.

---

## 15. Technology choices

| Concern | Choice | Note |
|---|---|---|
| Record store | `object_store` (via Lance; S3/GCS/Azure/local) | Create-only PUT per record; `git2` read-only for harvest |
| Record id / hash | `uuid` v7, `blake3` | JCS-canonical JSON per blob; NDJSON for bulk export |
| Signing | `ssh-key` (SSHSIG) | Optional per-record signature field |
| Derived store | LanceDB (existing; evaluate 0.40) | FTS + vectors exactly like `search` |
| Fingerprints | tree-sitter (bump to 0.27) | Hand-rolled blake3 memo; **not** salsa (experimental) |
| Watching | `notify` + `notify-debouncer-full` | For `comindly ui` / daemon live mode |
| MCP | `rmcp` 3.x | Structured output, resources, subscriptions |
| LLM (distiller only) | Rig (existing) | Optional; everything else is deterministic |
| TUI | `ratatui` 0.30, `crossterm`, `tui-tree-widget`, `ratatui-image` | Elm architecture |
| Web | `axum` 0.8, `tower-http`, `rust-embed`; Sigma.js + graphology, d3, @xyflow/react | Single binary, localhost-only |

Module layout extends the single crate: `src/ledger/{record,store,anchor,stale,retrieve,distill,
check,hook}.rs`, `src/ui/{tui,web}/`.

---

## 16. Open decisions

1. **Default write path for agent records:** drafts-first (human inbox) vs direct T1. *Recommend
   drafts-first; T1 direct only for explicit `remember` calls.*
2. **Compaction owner:** concurrency is safe either way; *recommend CI as the routine owner, with
   `comindly ledger compact` available to anyone.*
3. **Constraint enforcement default:** advisory vs blocking in CI. *Recommend advisory until T4
   precision is proven, then opt-in blocking per constraint.*
4. **rmcp 3 upgrade timing** — it is breaking; do it first (P0) rather than mid-feature.
5. **Name** for the feature in the CLI and docs ("ledger" internally; `why` as the user verb?).

---

## Sources

- Entire checkpoints: https://docs.entire.io/web/checkpoints · ref storage: https://entire.io/blog/introducing-ref-based-checkpoint-storage
- git-ai: https://usegitai.com/docs · Agent Trace: https://agent-trace.dev
- Lore protocol: https://arxiv.org/html/2603.15566v1
- GitHub Copilot agentic memory: https://github.blog/ai-and-ml/github-copilot/building-an-agentic-memory-system-for-github-copilot/
- Graphiti temporal model: https://mintlify.com/getzep/graphiti/concepts/temporal-model
- trace-mcp decision memory: https://trace-mcp.com/decision-memory.html
- Swimm auto-sync: https://swimm.io/blog/how-swimms-github-app-works
- Devin knowledge: https://docs.devin.ai/product-guides/knowledge.md
- claude-mem: https://docs.claude-mem.ai/architecture/overview
- Qodo rules: https://docs.qodo.ai/code-review/get-started/rule-enforcement/suggested-rules
- git-bug data model: https://github.com/git-bug/spec/
- Papers: VibeMemBench 2609.23570 · STALE 2605.06527 · SWE-ContextBench 2602.08316 ·
  Agent KB 2507.06229 · ACE (ICLR 2026) · MINJA 2503.03704 · Shared Org Memory 2608.00122
- Claude Code hooks: https://code.claude.com/docs/en/hooks · Cursor hooks: https://cursor.com/docs/hooks
- MCP spec 2026-07-28: https://modelcontextprotocol.io/specification/2026-07-28/changelog
- Lance versioning: https://docs.lancedb.com/tables/versioning · ratatui 0.30: https://ratatui.rs/highlights/v030/
