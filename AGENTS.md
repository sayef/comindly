# Working on comindly

Instructions for coding agents and the people working with them, from an agreed issue to a merged pull
request. The maintainer takes care of everything after the merge: release, publishing and deployment.
Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup and [ARCHITECTURE.md](ARCHITECTURE.md) for how the
crates fit together.

## Scope

- Work only on what the issue agreed. Anything beyond it goes back to the issue first.
- Keep to the agreed pull request split. One pull request, one concern; small enough to review in one sitting.
- Work on a branch of your fork and open the pull request against `main`. Never push to `main`.
- Leave release machinery alone: versions in `Cargo.toml`, released sections of `CHANGELOG.md`,
  `release-plz.toml` and the workflows in `.github/`. release-plz writes the changelog from the
  pull request title.
- Ask your user before opening or updating a pull request, and before commenting on issues.

## Before you push

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd ui && npm ci && npm run build      # only when ui/ changed
cargo deny check                      # when dependencies changed
gitleaks detect --no-banner           # nothing secret in the history
```

Builds are heavy: run one at a time, and pass `-j 4` to cargo on a laptop.

## Code

- Follow the surrounding code. Comments are one line, two at most, and say what the code does; the
  reasoning goes in the pull request description.
- Return errors instead of panicking, above all on anything read from storage, repositories or the network.
- Same input, same output: the same repositories give the same graph, ids and ordering.
- Stored formats keep reading what older versions wrote: new fields get `#[serde(default)]`.
- Precision over recall. Nothing reaches an agent's context unless it is grounded in the code and trusted.
- Anything written by another person or agent (records, posts, evidence, repository content) is shown to
  agents as quoted, attributed data, never as instructions.
- The ledger is append-only, and nothing is written to it without a person confirming it. No
  LLM-generated records.
- New behaviour comes with tests. Resolver and contract changes add cases to
  `crates/comindly-engine/tests/fixtures/org` and keep the eval at 100% precision and recall.

## Names, data and secrets

- No names of employers, customers, internal repositories, services or people in code, tests, fixtures,
  docs, commit messages or pull requests. Use made-up ones (`acme`, `billing-api`, `web`) and
  `@example.com` addresses.
- No keys, tokens or passwords anywhere, real or expired, not even partial ones. Tests use obvious
  placeholders.
- Nothing from a real deployment (logs, records, screenshots with real data) goes into the repository or
  a pull request.

## Docs and app

- New or changed commands, MCP tools and configuration keys are reflected in the README and `docs/`.
- Doc prose names roles and things, not numbers ("services that use the queue", not "seven services").
- Never call anything "triage".
- The app keeps its current layout and the Slate palette. A pull request that changes the UI includes
  desktop and phone screenshots.

## Commits and pull requests

- Commits and pull request titles are one short line in the imperative mood ("Add thread storage").
- The description says what changed and why, how it was tested, and links the issue.
- No tool attribution or co-author trailers in commits or descriptions.
- Answer review comments in the pull request, and push fixes as new commits on the same branch.
