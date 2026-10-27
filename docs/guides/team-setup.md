# Team setup

## One shared index

The index lives in **your** object store (or a local path). Nothing is sent anywhere else.

```bash
comindly link ../shared-lib ../service-a ../service-b --index-dir s3://your-bucket/comindly --no-llm
```

- Run it from CI on every merge to the default branch (see [CI](ci.md)); `--incremental`
  re-parses only changed files. Each run is a new, atomically readable version.
- What gets indexed: symbols and calls/imports across repos (Python, TypeScript/JavaScript, Go,
  Java, Kotlin, Rust; SCIP indexes when present), service contracts (HTTP routes and clients,
  SQS/SNS/S3, SQL tables), Terraform queues/topics/buckets and subscriptions, YAML config values,
  package versions, conventions and co-change recipes.
- `--no-llm` keeps indexing free and local; drop it to add LLM summaries, flow walkthroughs and
  a style guide (sends code to your configured LLM).

Each developer can run `comindly doctor` to check their setup (index, hooks, keys, signing).

## Give agents the index

```bash
comindly serve --index-dir s3://your-bucket/comindly        # MCP server (add to the agent's config)
comindly init-hooks claude                                # Claude Code hooks to paste into settings
```

- **MCP tools**: navigation (`search`, `find`, `zoom`, `thread`, `flow`), impact (`ripple`,
  `context_pack`, `preflight`, `contracts`), conventions (`guide`) and the ledger (`why`,
  `remember`, `review`).
- **Hooks** inject a short digest at session start and the records about a file just before an
  agent edits it (about 60 ms; a remote index is read from a local mirror refreshed in the
  background). Other agents: `comindly hook generic pre-edit` prints plain markdown.

## Team settings

Settings everyone on an index must share live with the index (`<index>/_team`, a versioned LanceDB
table), so every member, CI job and agent runs the same: the LLM endpoint and model, the embedding model
(an index's vectors only work with the model that made them) and which LLM steps run while indexing. They
are the only place these are set. Your own preferences (`comindly config`) keep what is personal: the
index location, the repositories your machine indexes, your name and signing.

The team settings hold no secrets. Anyone who can read the index can see them; access to the index is
the trust boundary.

Set them once, in the app (Settings → General, the Team card) or from the terminal:

```sh
comindly team set name="Platform" llm_model=gpt-6-luna enrich=true
comindly team                    # what is set, and where your API key comes from
```

Saving needs write access to the index (on a team server: an admin); two people saving at once cannot overwrite each other (the
second save is refused and asks to reload).

| Setting | What it does | Default |
|---|---|---|
| `llm_base_url`, `llm_model`, `embed_model` | The endpoint and models everyone uses | OpenAI, `gpt-6-luna`, `minishlab/potion-base-8M` |
| `embed`, `enrich`, `flows`, `guide` | Which steps run while indexing: search embeddings, summaries, flow walkthroughs, style guides | all on |
| `max_enrich` | Most symbols summarised, the most-used first | no limit |
| `max_flows` | Most flows given a walkthrough, those reaching furthest first | no limit |
| `llm_concurrency` | LLM requests at once; lower it if the provider rate-limits | 8 |
| `llm_price_in`, `llm_price_out` | The model's price in USD per million input and output tokens, so indexing shows its cost | not set |
| `format` | What agents get as text, `md` or `json`; every answer also carries the data as structured JSON | `md` |
| `scip` | Compiler-grade resolution: `auto` uses SCIP indexes when present, `require` fails without them, `off` uses tree-sitter only | `auto` |
| `auto_index` | Re-index when a listed repository's default branch moves on its remote (checked from the app or server) | off |
| `auto_index_llm` | Run the LLM steps on those automatic re-indexes too; only new or changed code is sent | off |

Summaries, walkthroughs and style guides are kept and reused while their input is unchanged, and saved as
they are written (summaries every 500), so an interrupted run continues where it stopped. Those of code
that changed are written again, and those of code that is gone are dropped. A style guide is
rewritten section by section, only where a measured ratio moved past the nearest ten percent or more
than a fifth of the section's other evidence differs from what it was written from. A server that restarts
without its clones does not re-index for that alone; it clones again only when a default branch moved.

### API keys

Everyone brings their own LLM API key. comindly looks for it, in order:

1. the entry for the team's name in `~/.config/comindly/credentials`,
2. the default key in that file,
3. `OPENAI_API_KEY`.

The file is created readable only by you. Set it from the app (Settings → General, under Your LLM key) or the terminal:

```sh
comindly team key              # prompts for the key; stored under the team's name
comindly team key --default    # your default key, for any team
```

```toml
# ~/.config/comindly/credentials
llm_api_key = "sk-..."       # default

[[team]]
name = "Platform"
llm_api_key = "sk-..."
```

CI sets `OPENAI_API_KEY` from its secret store.

### Starting a team

In the app, **Create a team** on the first screen takes a team name and an empty location everyone can
reach: a shared folder, or a bucket location (`s3://`, `gs://`, `az://`). comindly writes the team settings
there (which also checks you can write to it), switches to it and opens **Settings → Repositories** to add
and index the first repositories. It refuses a location that already has a team or an index; join that
one instead.

### Joining

A colleague needs only the index location and their own access to it ([Storage](storage.md)):

```sh
comindly join s3://bucket/comindly
```

or, in the app, **Join a team** on the first screen; this works as soon as
the team exists, before anything is indexed. For LLM steps they add
their own API key as above.

### With a team server

When the team runs a [comindly server](self-hosting.md), developers sign their command line in once
with `comindly login https://<server>`; from then on `find`, `why`, `remember` and the other reading and
recording commands ask the server, with each person's role ([Command line](../reference/cli.md#against-a-server)).
