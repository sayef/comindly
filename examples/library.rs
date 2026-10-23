//! The library guide's snippets: `cargo run --example library -- <index root>`.

use std::sync::{Arc, RwLock};

use comindly::graph::CodeGraph;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "s3://bucket/comindly".into());
    let (symbols, edges) = comindly::index::read_graph(&format!("{root}/_graph")).await?;
    let graph = CodeGraph::build(&symbols, &edges);

    for (repo, count) in graph.repos() {
        println!("{repo}: {count} symbols");
    }
    for node in graph.find("create_order", 5) {
        println!("{} {} at {}", node.kind, node.name, node.location);
    }

    if let Some(start) = graph.lookup("placeOrder") {
        for step in graph.flow(start, 8) {
            println!(
                "{}{} via {}",
                "  ".repeat(step.depth),
                step.node.name,
                step.via
            );
        }
    }
    for (entry, repos) in graph.entry_points(10) {
        println!("{} reaches {repos} repositories", graph.node(entry).name);
    }

    let ledger = comindly::ledger::Ledger::open(&root)?;
    for record in ledger.all().await? {
        println!("{:?}: {}", record.kind, record.title);
    }

    let fresh = std::time::Duration::from_secs(comindly::cache::DEFAULT_TTL);
    let server = comindly::mcp::load(&format!("{root}/_graph"), true, fresh).await?;
    let current = Arc::new(RwLock::new(Some(server)));
    let _app: axum::Router = axum::Router::new().nest_service(
        "/mcp",
        comindly::mcp::http(current.clone(), vec!["mcp.example.com".into()]),
    );
    Ok(())
}
