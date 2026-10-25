//! comindly's engine: cross-repo code intelligence as a library. Indexing runs as an async,
//! cancellable pipeline with structured events; everything a call needs is passed in.
//!
//! One module per stage: `model` (SCIP identity, symbols and edges) → `parse` (tree-sitter) →
//! `git` (incremental) → `resolve` (cross-repo binding) → `index` (LanceDB) → `graph` →
//! `embed` and `search` → `llm` (summaries, walkthroughs) → `ledger` (why code is as it is).

pub mod answers;
pub mod checks;
pub mod configvals;
pub mod context;
pub mod conventions;
pub mod docstore;
pub mod embed;
pub mod eval;
pub mod events;
pub mod facts;
pub mod git;
pub mod graph;
pub mod hosting;
pub mod index;
pub mod infra;
pub mod lang;
pub mod ledger;
pub mod llm;
pub mod manifests;
pub mod model;
pub mod packages;
pub mod parse;
pub mod pipeline;
pub mod preflight;
pub mod recipes;
pub mod resolve;
pub mod rt;
pub mod scip;
pub mod search;
pub mod signing;
pub mod storage;
pub mod styleguide;
pub mod team;
pub mod view;
pub mod worktree;
