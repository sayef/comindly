//! Flows follow a request across services: HTTP routes to their handlers, topics and queues to their subscribers.

use std::path::Path;

fn shop() -> comindly_engine::graph::CodeGraph {
    let (mut symbols, mut edges, mut facts) = (Vec::new(), Vec::new(), Vec::new());
    for repo in [
        "acme-common",
        "orders",
        "payments",
        "inventory",
        "storefront",
    ] {
        let out = comindly_engine::parse::parse_repo(
            &Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/acme")).join(repo),
            repo,
        )
        .unwrap();
        symbols.extend(out.symbols);
        edges.extend(out.edges);
        facts.extend(out.facts);
    }
    let resolved = comindly_engine::resolve::resolve(&symbols, &edges, &facts);
    symbols.extend(resolved.symbols);
    comindly_engine::graph::CodeGraph::build(&symbols, &resolved.edges)
}

fn reached(g: &comindly_engine::graph::CodeGraph, from: &str) -> Vec<(String, String)> {
    let start = g.lookup(from).expect("entry indexed");
    g.flow(start, 12)
        .into_iter()
        .map(|s| (s.node.repo, s.node.name))
        .collect()
}

#[test]
fn checkout_reaches_orders_and_inventory_over_http() {
    let g = shop();
    let steps = reached(&g, "placeOrder");
    let has = |repo: &str, name: &str| steps.iter().any(|(r, n)| r == repo && n == name);
    assert!(has("orders", "create_order"), "{steps:?}");
    assert!(has("inventory", "Store.reserve"), "{steps:?}");
}

#[test]
fn an_order_reaches_payments_through_the_topic_and_queue() {
    let g = shop();
    let steps = reached(&g, "create_order");
    let has = |name: &str| steps.iter().any(|(_, n)| n == name);
    assert!(has("sns:order-events"), "{steps:?}");
    assert!(has("sqs:order-payments"), "{steps:?}");
    assert!(
        steps.iter().any(|(r, n)| r == "payments" && n == "poll"),
        "{steps:?}"
    );
}
