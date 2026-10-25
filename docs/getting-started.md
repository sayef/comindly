# Getting started

comindly builds a map of your repositories: what calls what, across repositories and through HTTP
routes, queues, topics, buckets and tables, plus what your team recorded about the code. Your coding
agent reads that map through MCP; you read it in the terminal and a local web UI.

This page takes you from nothing to an agent that uses comindly, in about ten minutes.

## Ways to run comindly

| | For | Start here |
| --- | --- | --- |
| **On your machine** | One person: the CLI, the app and MCP over stdio for your agent | This page |
| **A team server** | A team: one container, members sign in, agents connect over HTTP | [Self-hosting](guides/self-hosting.md) |
| **A shared index from CI** | A team without a server: CI builds the index, everyone reads it | [Team setup](guides/team-setup.md) |
| **As a library** | Your own tools on top of the index, or comindly's MCP tools in your service | [Using comindly as a library](guides/library.md) |

All of them read the same index, so you can start on your machine and move to a server later.

## 1. Install

Prebuilt binaries for macOS (arm64, x64) and Linux (x64, arm64) are attached to each
[GitHub release](https://github.com/sayef/comindly/releases):

```sh
curl -LsSf https://raw.githubusercontent.com/sayef/comindly/main/scripts/install.sh | sh
comindly --version
```

From [crates.io](https://crates.io/crates/comindly), with Rust 1.95 or newer plus `protoc` and `cmake`:

```sh
cargo install comindly
```

Or from source:

```sh
git clone https://github.com/sayef/comindly && cd comindly && cargo build --release
```

## 2. Index your code

The quickest way is the app: `comindly app` (its own window on macOS, the browser elsewhere), then **Settings → Repositories**
→ Scan a folder → tick your repositories → **Index now**. The same from the terminal:

```sh
comindly repos add ../shared-lib ../orders ../payments   # remembered in your preferences
comindly repos add https://github.com/acme/billing.git    # a clone URL: comindly clones it itself
comindly link --index-dir ~/comindly-index                 # each repo's default branch, never your folders
```

Or one repository, as it is on disk, into the default location (`comindly config path` shows it):

```sh
comindly index .
```

Several repositories that talk to each other belong in one index, so comindly can connect them:

```sh
comindly link ../shared-lib ../orders ../payments --index-dir ~/comindly-index
```

Everything runs locally. With `OPENAI_API_KEY` set (or any OpenAI-compatible endpoint), comindly also
writes one-line summaries, flow walkthroughs and a style guide; `--no-llm` turns that off. Re-running
`link` is incremental: only repositories that changed are parsed again.

For a team, build the index in CI into your own bucket (`--index-dir s3://bucket/prefix`) and let
everyone read it; see [Team setup](guides/team-setup.md).

## 3. Look around

```sh
comindly search "where do we retry HTTP calls" --index-dir ~/comindly-index
comindly explore OrderService.place_order --index-dir ~/comindly-index   # neighbourhood and blast radius
comindly contracts --index-dir ~/comindly-index                         # who serves and who calls what
comindly app --index-dir ~/comindly-index                               # macOS: its own window
comindly ui --web --open --index-dir ~/comindly-index                   # any OS: in the browser
```

The web UI shows the org map, contracts, the ledger, conventions and internal packages; press
<kbd>⌘K</kbd> to search by name or by describing what you are looking for. Its **Get started** page
repeats the next step with your index path filled in.

## 4. Connect your agent

comindly serves MCP over stdio: the agent starts `comindly serve` and calls its tools when a question
needs them.

=== "Claude Code"

    ```sh
    claude mcp add --scope user comindly -- comindly serve --index-dir ~/comindly-index
    ```

=== "Cursor"

    `~/.cursor/mcp.json`:

    ```json
    {
      "mcpServers": {
        "comindly": { "command": "comindly", "args": ["serve", "--index-dir", "~/comindly-index"] }
      }
    }
    ```

=== "Codex"

    `~/.codex/config.toml`:

    ```toml
    [mcp_servers.comindly]
    command = "comindly"
    args = ["serve", "--index-dir", "~/comindly-index"]
    ```

=== "VS Code"

    `.vscode/mcp.json`:

    ```json
    {
      "servers": {
        "comindly": { "type": "stdio", "command": "comindly", "args": ["serve", "--index-dir", "~/comindly-index"] }
      }
    }
    ```

For an index in a bucket, the server needs the same storage credentials as you ([Storage](guides/storage.md)),
for example `-e AWS_PROFILE=…` with `claude mcp add`, or an `env` block in the JSON and TOML configs.

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

## 6. Check your setup

```sh
comindly doctor --index-dir ~/comindly-index
```

It checks the index, the ledger, cloud credentials, hooks, the LLM key, signing and SCIP indexers,
and says how to fix what is missing. The app's Get started page shows the same checks.

## Next

- [Using comindly with agents](guides/agents.md): the tools, hooks and good habits.
- [Team setup](guides/team-setup.md): one shared index from CI.
- [The ledger](guides/ledger.md): recording why code is the way it is.
- [Preflight](guides/preflight.md): checks before a commit or in MR pipelines.
