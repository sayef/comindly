//! Index a local repo into the cross-repo graph, then run graph queries — all in-process,
//! no network and no object storage. This is the deterministic core of comindly:
//! `parse` (tree-sitter) → `resolve` (bind edges) → `graph` (ripple / zoom).
//!
//! Run it against any Python/TypeScript repo:
//!
//! ```text
//! cargo run --example index_and_search -- ../some-repo            # picks the most-depended-on symbol
//! cargo run --example index_and_search -- ../some-repo Settings   # focus a specific symbol
//! ```

use comindly_engine::model::SymbolKind;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let repo = args.get(1).map(String::as_str).unwrap_or(".");
    let query = args.get(2).cloned().unwrap_or_default();

    // 1. Parse the repo with tree-sitter → symbols + provisional (unbound) edges.
    let name = Path::new(repo)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repo");
    let parsed = match comindly_engine::parse::parse_repo(Path::new(repo), name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("parse failed for {repo:?}: {e:#}");
            return ExitCode::FAILURE;
        }
    };

    // 2. Bind imports, calls and bases to real definitions (cross-repo aware).
    let resolved = comindly_engine::resolve::resolve(&parsed.symbols, &parsed.edges, &parsed.facts);
    let mut symbols = parsed.symbols.clone();
    symbols.extend(resolved.symbols);

    // 3. Build the in-memory dependency graph (code + contract nodes).
    let g = comindly_engine::graph::CodeGraph::build(&symbols, &resolved.edges);
    println!(
        "indexed {} symbols, {} edges",
        g.node_count(),
        g.edge_count()
    );
    for (r, n) in g.repos() {
        println!("  {r}: {n} symbols");
    }

    // 4. Choose a focus symbol: the query if given, else the most-depended-on definition.
    let focus_name = if !query.is_empty() {
        query.clone()
    } else {
        parsed
            .symbols
            .iter()
            .filter(|s| !matches!(s.kind, SymbolKind::File))
            .max_by_key(|s| g.dependents_count(&s.id.render()))
            .map(|s| s.name.clone())
            .unwrap_or_default()
    };

    let Some(node) = g.find(&focus_name, 1).into_iter().next() else {
        eprintln!("no symbol matching {focus_name:?}");
        return ExitCode::SUCCESS;
    };
    println!("\nfocus: {} ({}) @ {}", node.name, node.kind, node.location);
    let Some(idx) = g.lookup(&node.id) else {
        return ExitCode::SUCCESS;
    };

    // 5. ripple — who breaks if I change this? (reverse BFS over Calls/Imports).
    let hops = g.ripple(idx, 3);
    println!("\nripple — {} dependent(s):", hops.len());
    for h in hops.iter().take(15) {
        println!(
            "  depth {} via {:?}  {}  @ {}",
            h.depth, h.via, h.node.name, h.node.location
        );
    }

    // 6. zoom — the 360° neighborhood of the focus symbol.
    let z = g.zoom(idx);
    println!(
        "\nzoom — {} members, {} callers, {} callees, {} importers",
        z.members.len(),
        z.callers.len(),
        z.callees.len(),
        z.importers.len()
    );

    ExitCode::SUCCESS
}
