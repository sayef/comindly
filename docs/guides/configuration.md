# Configuration

Settings live with whoever they belong to:

| Whose | What | Where | Change it with |
|---|---|---|---|
| Yours, on this machine | which index to open, the repositories this machine indexes, your name on ledger records, signing | a versioned table in comindly's config folder | `comindly config` |
| The index's | models, LLM steps and limits, prices, output format, SCIP, automatic re-indexing, the team's repository list | with the index, shared by everyone using it | `comindly team set`, or Settings → General in the app |
| A server's | address, admins, sign-in, mail, secrets key | the environment of the deployment ([Self-hosting](self-hosting.md)) | the deployment |

Command-line flags win over everything.

## Your preferences

```bash
comindly config                          # show them, as TOML
comindly config set author_name="Ada Lovelace" record_names=false
comindly config export prefs.toml        # write them to a file
comindly config import prefs.toml        # fill what is not set yet from a file
comindly config import prefs.toml --override   # make the file's settings yours, as they are
comindly config history                  # every saved version, with when and by whom
comindly config path                     # where they are kept
```

Each change is saved as a new version. A file to import has the same shape `export` writes:

```toml
index_dir = "~/.local/share/comindly"   # folder, s3://, gs:// or az://
author_name = "Ada Lovelace"            # name on your ledger records (default: git user.name)
record_names = true                     # false: only a pseudonymous id on new records
# signing_key = "~/.ssh/id_ed25519.pub"           # sign ledger records (default: git user.signingkey)
# allowed_signers = "~/.config/git/allowed_signers"
# cache_ttl_secs = 300                  # how often the app, MCP server and hooks look for a new index

# The repositories `comindly link` indexes when given no folders.
[[repos]]
path = "~/src/orders"

[[repos]]
path = "~/.local/share/comindly/repos/github.com/acme/payments"
remote = "https://github.com/acme/payments.git"   # comindly clones it into `path` when missing
```

An import refuses keys it does not know, so a typo does not go unnoticed. Without `--override` it fills
only what is unset and names what it left as it was.

Earlier versions kept these in `~/.config/comindly/config.toml`. The first command after upgrading
imports that file once and renames it `config.toml.imported`; models and LLM settings in it move to the
index's shared settings. A `config.toml` that appears later is not read: comindly says so and points to
`comindly config import`.

## The index's shared settings

Everyone using an index runs with the same models and steps, so these are stored with the index:

```bash
comindly team                                    # show them
comindly team set llm_model=gpt-6-luna enrich=true max_flows=50
comindly team set scip=require auto_index=true
comindly team set llm_base_url=                  # an empty value removes a setting
```

Keys: `name`, `llm_model`, `llm_base_url`, `embed_model`, `embed`, `enrich`, `flows`, `guide`,
`max_enrich`, `max_flows`, `llm_concurrency`, `llm_price_in`, `llm_price_out`, `format` (`md` or `json`),
`scip` (`auto`, `require` or `off`) and `auto_index`. Where one is unset, comindly's default applies.
See [Team setup](team-setup.md#team-settings).

## Environment

- `COMINDLY_INDEX_DIR`: the index location, over your preferences.
- `COMINDLY_LLM_MODEL`, `COMINDLY_LLM_BASE_URL`, `COMINDLY_EMBED_MODEL`: used where the index's shared
  settings leave them unset.
- `COMINDLY_FORMAT`, `COMINDLY_SCIP`: over the index's shared settings, for this run.
- `COMINDLY_SIGNING_KEY`, `COMINDLY_ALLOWED_SIGNERS`, `COMINDLY_AUTHOR_NAME`, `COMINDLY_AGENT`: signing,
  and how your records and your agent's are named.
- `COMINDLY_SERVER`, `COMINDLY_TOKEN`: ask a comindly server, signed in with a personal access token
  (for CI; on your machine `comindly login` keeps the sign-in).
- `COMINDLY_LOG_FORMAT=json`: one JSON event per line, for platform logs.

## Secrets

Secrets are kept apart from settings. Your LLM API key comes from `OPENAI_API_KEY` or
`comindly team key`, which stores it in `~/.config/comindly/credentials`, readable only by you; that file
also holds the server sign-in from `comindly login`. On a server, admins set the LLM key and git tokens
under Settings → Integrations, encrypted with the deployment's key. Bucket access uses the platform's own
credentials ([Storage](storage.md)).
