//! What the apps show of an index — graph, contracts, ledger, conventions, dependencies — read once.

use crate::conventions::Convention;
use crate::graph::{CodeGraph, ContractView};
use crate::ledger::Entry;
use crate::manifests::InternalDep;
use crate::model::{Edge, Symbol};

/// Everything the UI shows, loaded from an index.
pub struct Data {
    pub graph: CodeGraph,
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
    pub contracts: Vec<ContractView>,
    pub entries: Vec<Entry>,
    /// Measured conventions; `manual` holds the team's changes to them.
    pub conventions: Vec<Convention>,
    pub manual: Vec<crate::conventions::Manual>,
    pub deps: Vec<InternalDep>,
    /// Index root for writing ledger reviews; `None` makes the UI read-only.
    pub root: Option<String>,
    /// Why the index could not be loaded (the UIs then start empty).
    pub problem: Option<String>,
}

impl Data {
    /// Everything the UIs show, read from the index at `root`.
    pub fn load(root: &str, verifier: &crate::signing::Verifier) -> anyhow::Result<Data> {
        let dir = format!("{}/_graph", root.trim_end_matches('/'));
        let (symbols, edges) = crate::index::read_graph_blocking(&dir)
            .map_err(|e| anyhow::anyhow!("no index at {dir}: {e:#}"))?;
        let mut conventions = match crate::index::read_facts_blocking(&dir) {
            Ok(Some(facts)) => crate::conventions::mine(&symbols, &edges, &facts),
            _ => Vec::new(),
        };
        conventions.extend(crate::index::read_recipes_blocking(&dir).unwrap_or_default());
        let manual = crate::conventions::manual_blocking(root).unwrap_or_default();
        let records = crate::ledger::all_blocking(root).unwrap_or_default();
        let mut entries = crate::ledger::derive(&records, &symbols, verifier);
        let packages = crate::index::read_packages_blocking(&dir).unwrap_or_default();
        let findings = crate::checks::run(&entries, &symbols, &edges, &packages);
        crate::checks::promote(&mut entries, &findings);
        let deps = crate::manifests::internal(&packages);
        let graph = CodeGraph::build(&symbols, &edges);
        Ok(Data {
            contracts: graph.contracts(None, false),
            graph,
            symbols,
            edges,
            entries,
            conventions,
            manual,
            deps,
            root: Some(root.to_string()),
            problem: None,
        })
    }

    /// No index yet: an empty view of `root` that says why.
    pub fn empty(root: &str, problem: String) -> Data {
        Data {
            graph: CodeGraph::build(&[], &[]),
            symbols: Vec::new(),
            edges: Vec::new(),
            contracts: Vec::new(),
            entries: Vec::new(),
            conventions: Vec::new(),
            manual: Vec::new(),
            deps: Vec::new(),
            root: Some(root.to_string()),
            problem: Some(problem),
        }
    }

    /// The index at `root`, or an empty view saying why it could not be read.
    pub fn load_or_empty(root: &str, verifier: &crate::signing::Verifier) -> Data {
        Data::load(root, verifier).unwrap_or_else(|e| Data::empty(root, format!("{e:#}")))
    }
}
