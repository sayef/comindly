# Getting started

comindly builds a map of your repositories: what calls what, across repositories and through HTTP
routes, queues, topics, buckets and tables, plus what your team recorded about the code. Your coding
agent reads that map through MCP; you read it in the terminal and in the app.

This page takes you from nothing to an agent that uses comindly, in about ten minutes.

## Ways to run comindly

| | For | Start here |
| --- | --- | --- |
| **On your machine** | One person: the CLI, the app and MCP over stdio for your agent, with the engine running locally | This page |
| **A team server** | A team: one container, members sign in, the CLI and agents connect to it | [Self-hosting](guides/self-hosting.md) |
| **A shared index from CI** | A team without a server: CI builds the index, everyone reads it | [Team setup](guides/team-setup.md) |
| **As a library** | Your own tools on top of the index, or comindly's MCP tools in your service | [Using comindly as a library](guides/library.md) |

All of them read the same index, so you can start on your machine and move to a server later.

## 1. Install

comindly comes as two programs: `comindly`, the command line, and `comindly-engine`, which holds the
index and does the work. Prebuilt binaries for macOS (arm64, x64) and Linux (x64, arm64) are attached
to each [GitHub release](https://github.com/sayef/comindly/releases):

```sh
curl -LsSf https://raw.githubusercontent.com/sayef/comindly/main/scripts/install.sh | sh
comindly --version
```

This installs the command line. The first command that needs the engine downloads it, of the same
version, and puts it beside `comindly` (`comindly engine install` does that now). For an offline
machine or CI, install both at once with `COMINDLY_ENGINE=1`; `COMINDLY_VERSION=v0.5.0-beta.7` picks a
version (betas are pre-releases, so the default is the latest stable one):

```sh
curl -LsSf https://raw.githubusercontent.com/sayef/comindly/main/scripts/install.sh | COMINDLY_ENGINE=1 sh
```

From [crates.io](https://crates.io/crates/comindly), with Rust 1.95 or newer plus `protoc` and `cmake`:

```sh
cargo install comindly comindly-engine
```

Or from source:

```sh
git clone https://github.com/sayef/comindly && cd comindly && cargo build --release -p comindly -p comindly-engine
```

Every command asks an engine. Out of the box that is `local`, the engine on this machine, started in
the background when a command needs it. To use a team server instead, see
[With a team server](guides/team-setup.md#with-a-team-server).

## 2. Index your code

The quickest way is the app: `comindly app` (its own window on macOS, the browser elsewhere), then **Settings → Repositories**
→ Scan a folder → tick your repositories → **Index now**. The same from the terminal:

```sh
comindly repos add ../shared-lib ../orders ../payments   # remembered in your preferences
comindly repos add https://github.com/acme/billing.git    # a clone URL: comindly clones it itself
comindly link                                             # each repo's default branch, never your folders
```

Or one repository, as it is on disk:

```sh
comindly index .
```

Several repositories that talk to each other belong in one index, so comindly can connect them:

```sh
comindly link ../shared-lib ../orders ../payments
```

The index goes to the engine's index location (`comindly config` shows it). To keep it somewhere else:

```sh
comindly config set index_dir=~/comindly-index
```

Everything runs locally. With `OPENAI_API_KEY` set (or any OpenAI-compatible endpoint), comindly also
writes one-line summaries, flow walkthroughs and a style guide; `--no-llm` turns that off. Re-running
`link` is incremental: only repositories that changed are parsed again.

For a team, build the index in CI into your own bucket (`index_dir=s3://bucket/prefix`) and let
everyone read it; see [Team setup](guides/team-setup.md).

## 3. Look around

```sh
comindly search "where do we retry HTTP calls"
comindly explore OrderService.place_order   # neighbourhood and blast radius
comindly contracts                          # who serves and who calls what
comindly app                                # its own window on macOS, the browser elsewhere
```

The app shows the org map, contracts, the ledger, conventions and internal packages; press
<kbd>⌘K</kbd> to search by name or by describing what you are looking for. Its **Get started** page
repeats the next step, ready to copy.

## 4. Connect your agent

comindly serves MCP over stdio: the agent starts `comindly serve` and calls its tools when a question
needs them. `comindly serve` forwards to the engine of the profile in use, so the same setup works
with the local engine and with a team server.

=== "Claude Code"

    ```sh
    claude mcp add --scope user comindly -- comindly serve
    ```

=== "Cursor"

    `~/.cursor/mcp.json`:

    ```json
    {
      "mcpServers": {
        "comindly": { "command": "comindly", "args": ["serve"] }
      }
    }
    ```

=== "Codex"

    `~/.codex/config.toml`:

    ```toml
    [mcp_servers.comindly]
    command = "comindly"
    args = ["serve"]
    ```

=== "VS Code"

    `.vscode/mcp.json`:

    ```json
    {
      "servers": {
        "comindly": { "type": "stdio", "command": "comindly", "args": ["serve"] }
      }
    }
    ```

For an index in a bucket, the local engine needs storage credentials ([Storage](guides/storage.md)). It
gets the environment of the command that started it, so start it from a shell that has them, for
example `AWS_PROFILE=… comindly engine start`.

Restart the agent and ask it something only a map of the whole codebase can answer:

- "What breaks in other services if I change the signature of `place_order`?"
- "Who calls `GET /orders/{id}`, and which queue does the order service publish to?"
- "What should I read before changing `PaymentClient.charge`?"
- "Why is this function written this way? Did anyone try something else?"

[Using comindly with agents](guides/agents.md) lists every tool with the questions it answers.

## 5. Optional: hooks

Hooks put what the team recorded in front of the agent without it asking: records about the
repository when a session starts, and about a file just before the agent edits it.

```sh
comindly init-hooks claude     # prints settings to merge into ~/.claude/settings.json
```

`comindly init-hooks cursor`, `gemini`, `codex` and `generic` print the equivalent for other agents.

To tell agents when to use comindly in a repository's own instructions, append
`comindly init-agents` to its `AGENTS.md` or `CLAUDE.md`; see
[Telling agents when to use it](guides/agents.md#telling-agents-when-to-use-it).

## 6. Check your setup

```sh
comindly doctor
```

It checks the profile in use, the local engine (installed, running, the same version), the sign-in,
the index, whether an agent has comindly's tools and whether Claude Code hooks are set, and says how
to fix what is missing. The app's Get started page shows the same checks.

## Next

- [Using comindly with agents](guides/agents.md): the tools, hooks and good habits.
- [Team setup](guides/team-setup.md): one shared index from CI.
- [The ledger](guides/ledger.md): recording why code is the way it is.
- [Preflight](guides/preflight.md): checks before a commit or in MR pipelines.
