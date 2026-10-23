# Using comindly as a library

The `comindly` crate is the same code the binary runs: reading an index, querying the graph, the
ledger and the MCP server. Use it to build your own tools on a team's index, or to serve comindly's
MCP tools from your own service. API reference: [docs.rs/comindly](https://docs.rs/comindly).

The library follows the binary's version. Until 1.0, minor versions may change it; pin one.

```sh
cargo add comindly
```

Building needs what `cargo install comindly` needs: Rust 1.95+, `protoc` and `cmake`.

## Read an index

An index root holds the graph under `_graph` and the ledger under `_ledger`, in a folder or a bucket
([Storage](storage.md)):

```rust
use comindly::graph::CodeGraph;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = "s3://bucket/comindly";
    let (symbols, edges) = comindly::index::read_graph(&format!("{root}/_graph")).await?;
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
let ledger = comindly::ledger::Ledger::open(root)?;
for record in ledger.all().await? {
    println!("{:?}: {}", record.kind, record.title);
}
```

`comindly::ledger::derive(&records, &symbols)` adds each record's status (active, stale, orphaned,
superseded) against the current code, as the app shows it.

## Serve the MCP tools from your own service

`mcp::load` reads an index into a server with every tool agents get from `comindly serve`. Serve it
over any rmcp transport, or mount the HTTP one in an axum router next to your own routes:

```rust
use std::sync::{Arc, RwLock};

let fresh = std::time::Duration::from_secs(comindly::cache::DEFAULT_TTL);
let server = comindly::mcp::load(&format!("{root}/_graph"), true, fresh).await?;
let current = Arc::new(RwLock::new(Some(server)));
let app = axum::Router::new().nest_service(
    "/mcp",
    comindly::mcp::http(current.clone(), vec!["mcp.example.com".into()]),
);
```

Replace what `current` holds after a re-index and new sessions get the new index. Put your own
authentication in front of `/mcp`; comindly's server mode does that with personal access tokens
([Self-hosting](self-hosting.md)).
