//! Browsing the index: overview, org map, symbols, search, packages, flows and source lines.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::doctor::Check;
pub use crate::graph::{ContractRole, ContractView, FlowStep, Node, Zoom};
pub use crate::view::View;

use super::Entry;

/// `GET /api/v1/overview`: the index at a glance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Overview {
    pub repos: Vec<RepoSymbols>,
    pub symbols: usize,
    pub edges: usize,
    pub cross_repo_edges: usize,
    pub contracts: usize,
    pub linked_contracts: usize,
    /// Ledger entries per status.
    pub ledger: BTreeMap<String, usize>,
    pub skew: Vec<Skew>,
    /// Whether this caller may write to the ledger and conventions.
    pub writable: bool,
    pub root: Option<String>,
    pub problem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSymbols {
    pub repo: String,
    pub symbols: usize,
}

/// A consumer locked to another version of a package than its publisher declares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skew {
    pub package: String,
    pub publisher: String,
    pub published: Option<String>,
    pub consumer: String,
    pub locked: Option<String>,
}

/// `GET /api/v1/orgmap?at=`: `at` indexes `/api/v1/versions` (oldest first); absent, the latest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtQuery {
    pub at: Option<usize>,
}

/// Repositories as nodes, linked by cross-repository code and contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMap {
    pub nodes: Vec<OrgNode>,
    pub edges: Vec<OrgEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgNode {
    pub id: String,
    pub name: String,
    pub repo: String,
    pub location: String,
}

/// `src` uses `dst`; `via` names the contracts a contract link goes through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgEdge {
    pub src: String,
    pub dst: String,
    /// `code` or `contract`.
    pub kind: String,
    pub weight: usize,
    pub via: Vec<String>,
}

/// `GET /api/v1/find?q=`: symbols by name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindQuery {
    pub q: String,
}

/// `GET /api/v1/node?focus=` and `/api/v1/context?focus=&budget=`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusQuery {
    pub focus: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<usize>,
}

/// One symbol: its neighbours, what a change ripples to, and what the team recorded about it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDetail {
    pub zoom: Zoom,
    pub graph: Neighbourhood,
    pub ripple: Ripple,
    pub ledger: Vec<Entry>,
    pub summary: Option<String>,
    pub flow: Option<Narration>,
}

/// Nodes and labelled edges around the focus, for the canvas graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Neighbourhood {
    pub nodes: Vec<Node>,
    pub edges: Vec<GraphEdge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub src: String,
    pub dst: String,
    pub kind: String,
}

/// How many symbols a change reaches, per repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ripple {
    pub total: usize,
    pub repos: BTreeMap<String, usize>,
}

/// A flow's walkthrough and the questions it answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Narration {
    pub narration: String,
    pub questions: Vec<String>,
}

/// `GET /api/v1/search?q=&repo=`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// A hybrid search hit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    pub location: String,
    pub signature: Option<String>,
    pub summary: Option<String>,
    pub dependents: usize,
}

/// `GET /api/v1/packages`: internal packages and who uses which version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub ecosystem: String,
    pub name: String,
    pub publisher: String,
    pub published: Option<String>,
    pub consumer: String,
    pub requirement: Option<String>,
    pub locked: Option<String>,
    pub skewed: bool,
}

/// `GET /api/v1/setup`: the index, its checks and how to connect agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setup {
    pub index: String,
    pub remote: bool,
    pub version: String,
    pub checks: Vec<Check>,
    pub tools: Vec<Tool>,
    pub agents: Vec<String>,
}

/// An MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
}

/// `GET /api/v1/flows`: an entry point and how far its flow reaches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRow {
    pub entry: Node,
    pub steps: usize,
    /// In the order the flow first reaches them.
    pub repos: Vec<String>,
    pub contracts: Vec<String>,
    pub summary: Option<String>,
    pub narrated: bool,
}

/// `GET /api/v1/flow?id=`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowQuery {
    pub id: String,
}

/// One flow: its walkthrough when the index has one, and every step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDetail {
    pub entry: Node,
    pub narration: Option<String>,
    pub questions: Option<Vec<String>>,
    pub steps: Vec<FlowStep>,
}

fn around() -> usize {
    6
}

/// `GET /api/v1/source?repo=&at=path:line&around=`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceQuery {
    pub repo: String,
    /// `path:line`, relative to the repository.
    pub at: String,
    #[serde(default = "around")]
    pub around: usize,
}

/// The lines around `path:line`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLines {
    pub path: String,
    pub line: usize,
    pub lines: Vec<SourceLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLine {
    pub n: usize,
    pub text: String,
}
