# Configuration

Settings live with whoever they belong to:

| Whose | What | Where | Change it with |
|---|---|---|---|
| Which engine you use | the local engine or a team server, and each one's sign-in | `~/.config/comindly/profiles.toml`, readable only by you | `comindly use`, `comindly login` |
| The local engine's | which index to open, the repositories this machine indexes, your name on ledger records, signing | a versioned table in comindly's config folder | `comindly config`, `comindly repos` |
| The index's | models, LLM steps and limits, prices, output format, SCIP, automatic re-indexing, the team's repository list | with the index, shared by everyone using it | `comindly team set`, or Settings → General in the app |
| A server's | address, admins, sign-in, mail, secrets key | the environment of the deployment ([Self-hosting](self-hosting.md)) | the deployment |

## Profiles

`comindly use` lists the engines the command line knows; `*` marks the one in use. `local` is the
engine on this machine and always exists. `comindly use https://comindly.example.com acme` adds a team
server as `acme` and signs in; `comindly use local` and `comindly use acme` switch. `--use <name>` on a
command, or `COMINDLY_PROFILE`, picks a profile for that command or shell only.

## The engine's settings

```bash
comindly config                          # show them, and where they are kept
comindly config set author_name="Ada Lovelace" record_names=false
comindly config set index_dir=s3://bucket/comindly   # point the local engine at a bucket index
```

Keys: `index_dir`, `author_name`, `record_names` and `cache_ttl_secs`. Each change is saved as a new
version. What is kept has this shape:

```toml
index_dir = "~/.local/share/comindly"   # folder, s3://, gs:// or az://
author_name = "Ada Lovelace"            # name on your ledger records (default: git user.name)
record_names = true                     # false: only a pseudonymous id on new records
# signing_key = "~/.ssh/id_ed25519.pub"           # sign ledger records (default: git user.signingkey)
# allowed_signers = "~/.config/git/allowed_signers"
# cache_ttl_secs = 300                  # how often the engine looks for a new index version

# The repositories `comindly link` indexes when given no folders.
[[repos]]
path = "~/src/orders"

[[repos]]
path = "~/.local/share/comindly/repos/github.com/acme/payments"
remote = "https://github.com/acme/payments.git"   # comindly clones it into `path` when missing
```

The repository list is changed with `comindly repos add|remove|scan` or in the app (Settings →
Repositories). `signing_key` and `allowed_signers` default to git's SSH signing settings; set them with
`COMINDLY_SIGNING_KEY` and `COMINDLY_ALLOWED_SIGNERS` in the engine's environment.

Earlier versions kept these in `~/.config/comindly/config.toml`. The engine imports that file once and
renames it `config.toml.imported`; models and LLM settings in it move to the index's shared settings. A
`config.toml` that appears later is not read.

## The index's shared settings

Everyone using an index runs with the same models and steps, so these are stored with the index:

```bash
comindly team                                    # show them
comindly team set llm_model=gpt-6-luna enrich=true max_flows=50
comindly team set scip=require auto_index=true auto_index_minutes=15
comindly team set llm_base_url=                  # an empty value removes a setting
```

Keys: `name`, `llm_model`, `llm_model_summary`, `llm_model_guide`, `llm_model_flow` (a model for
one task in place of `llm_model`), `llm_base_url`, `embed_model`, `embed`, `enrich`, `flows`, `guide`,
`max_enrich`, `max_flows`, `llm_concurrency`, `llm_price_in`, `llm_price_out`, `format` (`md` or `json`),
`scip` (`auto`, `require` or `off`), `auto_index`, `auto_index_llm` and `auto_index_minutes`. Where one is unset, comindly's default applies.
See [Team setup](team-setup.md#team-settings).

## Models and prompts

**Settings → Models & prompts** holds the LLM endpoint and model, a model per task (for example a
cheaper one for summaries and a stronger one for style guides), the embedding model, the LLM steps
and their limits, and the instructions indexing gives the LLM for summaries, style guides and
walkthroughs. All of it is stored with the index.

![Models & prompts: the LLM setup and the prompts on one page](../assets/screenshots/models.webp#only-light)
![Models & prompts: the LLM setup and the prompts on one page](../assets/screenshots/models-dark.webp#only-dark)
/// caption
Models & prompts: the LLM setup and the prompts on one page
///

Each task starts with comindly's built-in prompt. Saving an edit makes a new version that nothing
uses yet; for style guides it can also change what each section covers. **Try** runs a draft on one
symbol, flow or style guide section and shows it next to what the index holds now. Activating a
version, or deactivating it to go back to the built-in prompt, applies to what the next index
writes, partial or full; the same goes for a model change. Nothing already written is redone for
it: a partial index writes new or changed code with it, and a full index writes everything it
does not reuse. To write everything again with the current models and prompts, use **Rewrite
everything** on the same page, or `comindly link --rewrite summary,guide,flow`; it costs a full LLM
pass for each task chosen. Only the instructions can be changed: the reply format
and its JSON schema are always added, so a model or server without structured output still replies
in the right shape. When a release changes a built-in prompt that an active version replaces, the
page says so. On a team server, admins change models and prompts, everyone can read them, and
changes to prompts appear in the audit log.

From the terminal:

```bash
comindly prompts                          # what each task uses, and its versions
comindly prompts show guide               # the prompt in use; `show guide 0` is the built-in one
comindly prompts save summary draft.md --note "plainer words"
comindly prompts activate summary 2       # or: deactivate summary, delete summary 1
```

## Environment

- `COMINDLY_INDEX_DIR`: the index location, over the engine's settings.
- `COMINDLY_PROFILE`: the profile commands use, over the one `comindly use` chose.
- `COMINDLY_LLM_MODEL`, `COMINDLY_LLM_BASE_URL`, `COMINDLY_EMBED_MODEL`: used where the index's shared
  settings leave them unset.
- `COMINDLY_FORMAT`, `COMINDLY_SCIP`: over the index's shared settings, for this run.
- `COMINDLY_SIGNING_KEY`, `COMINDLY_ALLOWED_SIGNERS`, `COMINDLY_AUTHOR_NAME`, `COMINDLY_AGENT`: signing,
  and how your records and your agent's are named.
- `COMINDLY_SERVER`, `COMINDLY_TOKEN`: ask a comindly server, signed in with a personal access token,
  in place of a profile (for CI; on your machine `comindly use` keeps the sign-in).
- `COMINDLY_LOG_FORMAT=json`: one JSON event per line, for platform logs.

## Secrets

Secrets are kept apart from settings. Your LLM API key comes from `OPENAI_API_KEY` or
`comindly team key`, which stores it in `~/.config/comindly/credentials`, readable only by you. Sign-ins
to servers are kept in `~/.config/comindly/profiles.toml`, also readable only by you. On a server, admins set the LLM key and git tokens
under Settings → Integrations, encrypted with the deployment's key. Bucket access uses the platform's own
credentials ([Storage](storage.md)).
