# Using comindly as a library

The library is the same code the engine runs: indexing, reading an index, querying the graph, the
ledger and the MCP server. Use it to build your own tools on a team's index, to serve comindly's
MCP tools from your own service, or to talk to a running engine. It comes in these crates:

| Crate | What it is |
|---|---|
| `comindly-core` | Indexing and queries as a library: parsing, cross-repo binding, the graph, the index, search, LLM steps, the ledger and indexing as an async pipeline. Reads no environment or files of its own: everything a call needs is passed in. |
| `comindly-api` | The requests and answers of an engine's `/api/v1`. With the `client` feature, a typed client for it, as the command line uses it. |
| `comindly-engine` | The engine: the app and its `/api/v1`, MCP over HTTP, indexing jobs, accounts and roles, and the backends behind them, plus the `comindly-engine` binary. Re-exports core, so `comindly_engine::graph` is `comindly_core::graph`. |
| `comindly-ui` | The web app, built and embedded. |
| `comindly` | The command line only; it is not a library. |

API reference: [docs.rs/comindly-engine](https://docs.rs/comindly-engine). The crates follow the
binaries' version. Until 1.0, minor versions may change them; pin one.

```sh
cargo add comindly-engine   # or comindly-core for indexing and queries alone
```

Building needs what `cargo install comindly-engine` needs: Rust 1.95+, `protoc` and `cmake`.

## Read an index

An index root holds the graph under `_graph` and the ledger under `_ledger`, in a folder or a bucket
([Storage](storage.md)):

```rust
use comindly_engine::graph::CodeGraph;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = "s3://bucket/comindly";
    let (symbols, edges) = comindly_engine::index::read_graph(&format!("{root}/_graph")).await?;
    let graph = CodeGraph::build(&symbols, &edges);

    for (repo, count) in graph.repos() {
        println!("{repo}: {count} symbols");
    }
    for node in graph.find("create_order", 5) {
        println!("{} {} at {}", node.kind, node.name, node.location);
    }
    Ok(())
}
```

## Follow a flow across repositories

`flow` walks from a symbol through calls and across HTTP routes, topics and queues into the services
that handle them; `entry_points` ranks where flows start by how many repositories they reach:

```rust
if let Some(start) = graph.lookup("placeOrder") {
    for step in graph.flow(start, 8) {
        println!("{}{} via {}", "  ".repeat(step.depth), step.node.name, step.via);
    }
}
for (entry, repos) in graph.entry_points(10) {
    println!("{} reaches {repos} repositories", graph.node(entry).name);
}
```

## Read the ledger

```rust
let ledger = comindly_engine::ledger::Ledger::open(root)?;
let records = ledger.all().await?;
for record in &records {
    println!("{:?}: {}", record.kind, record.title);
}
let verifier = comindly_engine::signing::Verifier::default();
for entry in comindly_engine::ledger::derive(&records, &symbols, &verifier) {
    println!("{:?}: {}", entry.status, entry.record.title);
}
```

`derive` adds each record's status (active, stale, orphaned, superseded) against the current code, as
the app shows it. `Verifier::new(allowed_signers, cache)` also checks records' signatures; the default
checks none.

## Index repositories

Indexing is an async pipeline that reports its steps, results and progress as events to a sink of
yours, and stops at its next checkpoint when its cancellation token is cancelled. Without an LLM client
the LLM steps are skipped, and say so:

```rust
use std::sync::Arc;
use comindly_engine::context::{IndexContext, Settings};
use comindly_engine::events::{Event, EventSink};
use comindly_engine::pipeline::{self, Source};

struct Print;

impl EventSink for Print {
    fn emit(&self, e: Event) {
        eprintln!("{:?} {}", e.kind, e.message);
    }
}

let settings = Settings::resolve(&Default::default(), |_| None);
let ctx = IndexContext::new(root, settings, Arc::new(Print), "/tmp/comindly-cache".into());
let sources = vec![Source::at("../orders"), Source::at("../payments")];
pipeline::link(&ctx, sources, false, comindly_engine::scip::Mode::Auto).await?;
```

`Settings::resolve` takes the index's shared settings (`comindly_engine::team::load(root)`) and a lookup for
overrides; `IndexContext::with_llm` and `with_git` add an LLM client and git credentials.

## Serve the MCP tools from your own service

`mcp::load` reads an index into a server with every tool agents get from `comindly serve`. Serve it
over any rmcp transport, or mount the HTTP one in an axum router next to your own routes:

```rust
use std::sync::{Arc, RwLock};

let fresh = std::time::Duration::from_secs(comindly_engine::cache::DEFAULT_TTL);
let server = comindly_engine::mcp::load(&format!("{root}/_graph"), true, fresh).await?;
let current = Arc::new(RwLock::new(Some(server)));
let app = axum::Router::new().nest_service(
    "/mcp",
    comindly_engine::mcp::http(current.clone(), vec!["mcp.example.com".into()]),
);
```

Replace what `current` holds after a re-index and new sessions get the new index. Put your own
authentication in front of `/mcp`; comindly's server mode does that with personal access tokens
([Self-hosting](self-hosting.md)).
