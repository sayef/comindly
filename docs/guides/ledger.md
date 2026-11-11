# The ledger

What the code does not say.

Records are short, typed facts anchored to symbols: an `attempt` that failed or was reverted, a
`decision` with its rejected alternatives, a `constraint`, a `gotcha`, an `intent`, a `question`.
They follow the code and stay active until someone archives, invalidates or supersedes them. When
their code moves, is renamed or changes, a record gets a note saying so (**Code changed** in the
app), with the commit it saw; when all of its code is gone it is **orphaned**. Each anchor stores the indexed commit it was written against, and a
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
(`why` shows everything, with its trust badge). Every record and every change to one carries a
reason (a record's body, a review's `--reason`), so the next agent or person can see why and fix it:

```bash
comindly ledger --inbox                       # orphaned, disputed and likely repeats
comindly review <id> confirm --reason "…"     # still true: what you checked
comindly review <id> archive --reason "…"     # was true, no longer applies (kept as history)
comindly review <id> invalidate --reason "…"  # was wrong (kept, marked invalid)
comindly review <id> supersede --reason "…"   # or: dispute
comindly erase <id> --yes                     # remove permanently, with its reviews
comindly app                                  # the same, with a timeline, in the app
```

Agents use the same actions through the MCP `review` tool, with their person's OK for recording,
archiving, invalidating and superseding; deleting is for people only (CLI and app). Agents are asked
to confirm a record whenever they relied on it and checked it still matches the code: each person whose
agent (or who themselves) confirmed it counts once, and cards show how many. A record a person wrote
or confirmed, in the app or through their agent with `from_person`, is the team's rule (T3). A
confirmation also vouches for the code as indexed then, so its notes clear until that code changes
again. Reviewing in the app is optional.

Before an agent's record is written, it is compared with the standing records on the same symbols or
files. If one reads alike, nothing is written and the agent is pointed at it: it confirms that record
(adding what it knows in the reason), or writes its own anyway with `distinct`, saying how it differs.
That note is kept on the record as **Differs from**.
Records that already repeat an earlier one on the same code (written in the app, or before this check)
wait in **Needs review** as a **Possible repeat**: **Same as earlier** asks what the earlier record
already says and archives the later one, and **Keep both** confirms it.
A review's reason can run to a few paragraphs; titles stay short enough to read in a list.
Every record and review shows who made it and when: the person's name (config `author_name`, else git
`user.name`), and for agents also the agent ("claude-code for Ada Lovelace", taken from the MCP client
or `COMINDLY_AGENT`). Set `record_names = false` (`comindly config set record_names=false`) to store
only the pseudonymous actor id.

Nothing is mined from git history or merge requests. A `dispute` leaves the record active and marks
it **challenged** at the top of the inbox until someone archives, invalidates or confirms it.

Ten minutes a week of review is what keeps the ledger worth reading. Agent session transcripts
are never read.

## Trust and privacy

- **Signing**: with an SSH signing key configured (`signing_key`, or git `user.signingkey` with
  `gpg.format=ssh`), every record is signed; with `allowed_signers` configured, a record that was
  edited or signed by an unknown key drops to T0 and is never shown to agents.
- **People**: records describe code, never people; actors are pseudonymous ids.
- **Erasure**: `comindly erase <id…> --yes` deletes records for good, including old snapshot
  versions.
- **Secrets**: records that look like they contain a secret are refused; LLM input is redacted.
- **Poisoning**: records not written by a person are refused when they read like instructions to
  an agent or carry a shell or script payload; a record planted straight into the store with such
  content (or a secret) is treated as T0 and never injected.
