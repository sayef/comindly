# CI and merge requests

**Model.** One central pipeline checks out the team's repos, runs `comindly link …` (with
`--no-llm` when there is no LLM key), and writes a new **atomically-versioned** LanceDB dataset.
Lance's newest version manifest is the "latest" pointer every consumer reads — no coordination, no
staleness. comindly needs **no remote git credentials** itself when the CI job clones the repos (with its
token); comindly indexes the local checkouts and their `.git`.

The command line starts the local engine beside it and indexes through it, so CI needs both programs:
build them (`cargo build --release -p comindly -p comindly-engine`) or install them with
`COMINDLY_ENGINE=1` ([Getting started](../getting-started.md#1-install)). The output location
(`comindly config set index_dir=…`) is either a **local directory** (published as a downloadable CI
artifact) or a bucket location (**`s3://`**, **`gs://`** or **`az://`**). The shipped GitHub workflow uses the local-path + artifact
approach; the GitLab example below uses S3. Both are equivalent.

## GitHub Actions (shipped)

[`.github/workflows/comindly-index.yml`](https://github.com/sayef/comindly/blob/main/.github/workflows/comindly-index.yml) builds the index on a
schedule (and on demand). To enable it:

- **Variable** `COMINDLY_INDEX_ENABLED = true` — lets the schedule run.
- **Variable** `COMINDLY_REPOS` — space/newline list of `owner/repo` to index (kept in settings, not
  committed, so repo names stay out of the public workflow file).
- **Secret** `COMINDLY_REPO_TOKEN` — a fine-grained PAT with `Contents: read` (only for **private**
  repos; omit for public).
- **Secret** `OPENAI_API_KEY` *(optional)* — turns on the LLM steps.

It clones the listed repos, runs `comindly link …` (incremental against the previous index), and
uploads the resulting index directory (which contains the internal `_graph` dataset) as the
`comindly-index` artifact. Consume it with:

```bash
gh run download -R <owner>/comindly -n comindly-index   # → ./ (contains _graph)
comindly config set index_dir=$PWD
comindly search "…"
```

## Member repos trigger a reindex on push to main

Each product repo can fire the central pipeline when it merges to `main`.

**GitHub** (member repo → `notify-comindly.yml`), matching the `repository_dispatch` hook in
`comindly-index.yml`:

```yaml
on: { push: { branches: [main] } }
jobs:
  notify:
    runs-on: ubuntu-latest
    steps:
      - run: |
          curl -sf -X POST \
            -H "Authorization: token ${{ secrets.COMINDLY_DISPATCH_TOKEN }}" \
            -H "Accept: application/vnd.github+json" \
            https://api.github.com/repos/<owner>/comindly/dispatches \
            -d '{"event_type":"comindly-reindex"}'
```

**GitLab** (member repo `.gitlab-ci.yml`) — trigger the comindly project's pipeline via a
[pipeline trigger token](https://docs.gitlab.com/ci/triggers/):

```yaml
comindly-reindex:
  stage: deploy
  rules: [{ if: '$CI_COMMIT_BRANCH == "main"' }]
  script:
    - curl -sf -X POST -F token=$COMINDLY_TRIGGER_TOKEN -F ref=main
        "$CI_SERVER_URL/api/v4/projects/$COMINDLY_PROJECT_ID/trigger/pipeline"
```

## GitLab indexing pipeline (S3 variant)

```yaml
index:
  image: rust:1.97
  variables:
    COMINDLY_S3_URI: s3://YOUR-BUCKET/lancedb/org
    AWS_REGION: us-east-1
  rules:
    - if: '$CI_PIPELINE_SOURCE == "trigger"'
    - if: '$CI_PIPELINE_SOURCE == "schedule"'
  cache:
    key: { files: [Cargo.lock] }
    paths: [.cargo/registry, target]
  before_script:
    - apt-get update && apt-get install -y protobuf-compiler cmake
    # Clone member repos with full history (deploy/job token, read scope).
    - git clone "https://oauth2:${COMINDLY_REPO_TOKEN}@gitlab.com/<group>/pkg-common.git" repos/pkg-common
    - git clone "https://oauth2:${COMINDLY_REPO_TOKEN}@gitlab.com/<group>/service-a.git" repos/service-a
    # … more repos
  script:
    - cargo build --release -p comindly -p comindly-engine
    - ./target/release/comindly config set "index_dir=$COMINDLY_S3_URI"
    - ./target/release/comindly link repos/* --no-llm
    - ./target/release/comindly engine stop
```

Secrets: `COMINDLY_REPO_TOKEN` (read repos), AWS creds (OIDC role or keys), and an LLM key if you drop
`--no-llm`.

## SCIP (compiler-grade resolution)

The team setting `scip` defaults to `auto`: a repo's `index.scip` (any depth ≤ 4, gitignored is fine)
is ingested; otherwise any SCIP indexer on `PATH` that matches the repo's manifests runs
(`scip-typescript`, `scip-python`, `scip-go`, `scip-java`, `rust-analyzer scip`). Indexers need the
repo's dependencies installed, so install them before indexing. `scip=require` fails the job when
a repo yields no SCIP; `scip=off` skips it. Tree-sitter resolution covers everything SCIP doesn't.

## LLM provider (for summaries and walkthroughs)

Enrichment uses an LLM via Rig — provider-agnostic. Defaults to OpenAI (`OPENAI_API_KEY`, model
`gpt-6-luna`). Set `COMINDLY_LLM_MODEL` and `COMINDLY_LLM_BASE_URL` to target any OpenAI-compatible
endpoint (a LiteLLM proxy, Ollama, vLLM, Azure).

## Incremental indexing

`comindly index` and `comindly link` diff the last-indexed commit (stored in the Lance `repo_meta`
table) against HEAD and reparse only changed files, recomputing only the symbols in changed files.
Which steps run (embeddings, summaries, walkthroughs, style guides) is up to the
[team settings](team-setup.md#team-settings); `--no-llm` skips the LLM steps for one run.

## Merge-request preflight

Run `preflight` in each member repo's MR pipeline against the shared index and post the report as
an MR note. It lists recorded constraints and dead ends on the touched code, contracts other
services consume, blast radius, missing co-changes and failing constraint checks. `--strict` fails
the job when a `must` constraint check is violated in a changed file. Checks run on the shared
index, so they see the default branch plus the MR's changed paths, not edges new in the MR.

```yaml
comindly-preflight:
  stage: test
  rules: [{ if: '$CI_PIPELINE_SOURCE == "merge_request_event"' }]
  script:
    - comindly preflight . --base "$CI_MERGE_REQUEST_DIFF_BASE_SHA" --strict > preflight.md || STATUS=$?
    - |
      jq -Rs '{body: .}' preflight.md | curl -sf -X POST \
        -H "PRIVATE-TOKEN: $COMINDLY_NOTE_TOKEN" -H "Content-Type: application/json" \
        --data @- "$CI_API_V4_URL/projects/$CI_PROJECT_ID/merge_requests/$CI_MERGE_REQUEST_IID/notes"
    - exit ${STATUS:-0}
```

The job asks a team server with `COMINDLY_SERVER` and a personal access token in `COMINDLY_TOKEN`, or
the local engine pointed at the shared index (`comindly config set index_dir=…`).

After linking, the scheduled index job can also run, against the same index:

```bash
comindly ledger --check       # fail on violated `must` rules
```
