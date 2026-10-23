//! The library guide's snippets: `cargo run --example library -- <index root>`.

use std::sync::{Arc, RwLock};

use comindly::context::{IndexContext, Settings};
use comindly::events::{Event, EventSink};
use comindly::graph::CodeGraph;
use comindly::pipeline::{self, Source};

/// Where indexing reports its steps, results and progress.
struct Print;

impl EventSink for Print {
    fn emit(&self, e: Event) {
        eprintln!("{:?} {}", e.kind, e.message);
    }
}

/// Index two repositories into `root`, cross-repo links included.
#[allow(dead_code)]
async fn index(root: &str) -> anyhow::Result<()> {
    let settings = Settings::resolve(&Default::default(), |_| None);
    let ctx = IndexContext::new(
        root,
        settings,
        Arc::new(Print),
        "/tmp/comindly-cache".into(),
    );
    let sources = vec![Source::at("../orders"), Source::at("../payments")];
    pipeline::link(&ctx, sources, false, comindly::scip::Mode::Auto).await?;
    Ok(())
}

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
    let records = ledger.all().await?;
    for record in &records {
        println!("{:?}: {}", record.kind, record.title);
    }
    let verifier = comindly::signing::Verifier::default();
    for entry in comindly::ledger::derive(&records, &symbols, &verifier) {
        println!("{:?}: {}", entry.status, entry.record.title);
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
