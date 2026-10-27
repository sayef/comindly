# The ledger

What the code does not say.

Records are short, typed facts anchored to symbols: an `attempt` that failed or was reverted, a
`decision` with its rejected alternatives, a `constraint`, a `gotcha`, an `intent`, a `question`.
They follow the code: moved code keeps its records, changed code marks them **stale**, deleted
code marks them **orphaned**. Each anchor stores the indexed commit it was written against, and a
record can carry up to three short snippets of the code that shaped it, so a reader can see what was
true then:

```bash
comindly remember decision --on parse_order --title "Strip whitespace from order ids" \
  --snippet app/orders.py:12-20
```

Where records come from:

| Source | How | Trust |
|---|---|---|
| A person | `comindly remember constraint --on get_user --title "…"` | T3 |
| An agent | MCP `remember` (an attempt needs evidence) | T1–T2 |

Hooks and the ledger sections of `zoom`/`context_pack` only use records at **T2 or higher**
(`why` shows everything, with its trust badge). Proposals (T1) wait in the inbox until a person
confirms them:

```bash
comindly ledger --inbox                       # stale, orphaned and unconfirmed records
comindly review <id> confirm                  # still true
comindly review <id> archive --reason "…"     # was true, no longer applies (kept as history)
comindly review <id> invalidate --reason "…"  # was wrong (kept, marked invalid)
comindly review <id> supersede                # or: dispute
comindly erase <id> --yes                     # remove permanently, with its reviews
comindly ui --web                             # the same, with a timeline, in the browser
```

Agents use the same actions through the MCP `review` tool; deleting is for people only (CLI and web UI).
Every record and review shows who made it and when: the person's name (config `author_name`, else git
`user.name`), and for agents also the agent ("claude-code for Ada Lovelace", taken from the MCP client
or `COMINDLY_AGENT`). Set `record_names = false` to store only the pseudonymous actor id; `comindly erase
--actor` removes everything one person wrote.

Nothing is mined from git history or merge requests. Only a person can retire a human-backed record
(written or confirmed by a person). When an agent archives, invalidates or supersedes one, the record
stays active, is marked **challenged**, and goes to the top of the inbox. Machine-written records can
be retired by machines.

Ten minutes a week of review is what keeps the ledger worth reading. Agent session transcripts
are never read.

## Trust and privacy

- **Signing**: with an SSH signing key configured (`signing_key`, or git `user.signingkey` with
  `gpg.format=ssh`), every record is signed; with `allowed_signers` configured, a record that was
  edited or signed by an unknown key drops to T0 and is never shown to agents.
- **People**: records describe code, never people; actors are pseudonymous ids.
- **Erasure**: `comindly erase <id> | --actor me --yes` deletes records for good, including old
  snapshot versions.
- **Secrets**: records that look like they contain a secret are refused; LLM input is redacted.
- **Poisoning**: records not written by a person are refused when they read like instructions to
  an agent or carry a shell or script payload; a record planted straight into the store with such
  content (or a secret) is treated as T0 and never injected.
