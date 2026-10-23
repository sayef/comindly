# Contributing to comindly

Thanks for your interest in comindly. Bug reports, language and framework support, documentation fixes and
code are all welcome.

## Before you start

- For anything beyond a small fix, open an issue first so we can agree on the approach.
- Security issues go through [SECURITY.md](SECURITY.md), not public issues.
- By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).

## Development setup

You need Rust 1.95 or newer (the minimum supported version, checked in CI), plus `protoc` and `cmake`:

```sh
brew install protobuf cmake                          # macOS
sudo apt-get install -y protobuf-compiler cmake      # Debian/Ubuntu

git clone https://github.com/sayef/comindly && cd comindly
cargo test --workspace
cargo run -q -- eval graph -v                        # resolver quality on the fixture org
```

Tests that need a bucket or an LLM key are opt-in; everything else runs offline.

The web UI lives in `ui/` (React, TypeScript, Vite), which is also the `comindly-ui` crate that embeds its
build into the binary. Without
a build, `comindly ui --web` serves a placeholder page. With Node 24:

```sh
cd ui && npm ci && npm run build && cd ..            # embedded on the next cargo build
comindly ui --web &                                    # API on 127.0.0.1:7878; open its printed link once
cd ui && npm run dev                                 # hot reload on 127.0.0.1:5173, /api proxied
```

## Checklist for a pull request

- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings` are clean.
- UI changes build with `npm run build` in `ui/` (type-checked).
- `cargo deny check` passes (licences, advisories and sources; install with `cargo install cargo-deny --locked`).
- New behaviour has tests. Resolver, contract and infrastructure changes add cases to
  `tests/fixtures/org` with gold edges in `expected.toml`, and `comindly eval graph` stays at 100%
  precision and recall.
- Real-world changes are checked on real repositories as well: no new spurious contracts, and a note in
  the pull request of what changed.
- Performance changes include criterion results (`cargo bench --bench graph`); CI also posts a
  base-against-head comparison to the job summary of each pull request.
- Determinism holds: the same repositories give the same graph, ids and ordering.
- Changes to stored formats (Lance tables, ledger records, facts) keep reading what older versions wrote
  (`#[serde(default)]` on new fields).
- Commands, MCP tools and configuration keys are reflected in the README, [docs](https://comindly.pages.dev/) and the
  [changelog](CHANGELOG.md) (under `## [Unreleased]`).

## Project layout

A cargo workspace (see [ARCHITECTURE.md](ARCHITECTURE.md)):

- `crates/comindly-core`: the engine, one module per pipeline stage. It reads no environment or files of
  its own; anything it needs is passed in. Language support lives in `src/lang/` behind the `LangSpec`
  trait; contract linking in `src/resolve/contracts.rs`.
- `crates/comindly-server`: the web app's API, MCP, jobs, accounts and roles, and this machine's
  preferences and credentials.
- `ui/`: the web app, and the `comindly-ui` crate that embeds it.
- The root: the `comindly` command, the terminal UI and the macOS window. Integration tests, fixtures,
  benches and examples live here too.

## Style

- Follow the surrounding code. Keep comments short: say what the code does, and put the reasoning in the
  pull request.
- Prefer returning errors to panicking on bad input, especially on anything read from storage or
  repositories.
- Precision over recall: an unresolved edge is better than a wrong one, and nothing reaches an agent's
  context unless it is grounded and trusted.
- Measure before optimising, and include the numbers in the pull request.

## Releasing

Every push to `main` opens or updates a release pull request (version bump and changelog entry). Merging
it publishes nothing. To release, run the **Release-plz** workflow by hand (Actions → Release-plz → Run
workflow, or `gh workflow run release-plz.yml`): it publishes the four crates at one version, in
dependency order, tags `v<version>`, creates the GitHub release, and the Release workflow attaches the
binaries.

## Commit messages

One short line in the imperative mood ("Add Express route detection"). The pull request description says
why.
