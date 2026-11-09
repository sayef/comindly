use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::conventions::{Convention, Manual};
use crate::graph::{ContractView, Node};
use crate::search::SearchHit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Search {
    pub hits: Vec<SearchHit>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Why {
    pub name: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub markdown: String,
    /// Said instead of a report, when there is nothing to report.
    pub note: Option<String>,
    /// With `check`: a constraint that must hold is violated.
    pub failed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub focus: String,
    /// The stored walkthrough and the questions it answers, when one was written.
    pub narration: Option<(String, Vec<String>)>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub depth: usize,
    pub via: String,
    pub name: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contracts {
    /// Every contract, before `kind` and `dangling` narrow the list.
    pub total: usize,
    pub views: Vec<ContractView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conventions {
    pub conventions: Vec<Convention>,
    pub hidden: Vec<Manual>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explore {
    pub symbols: usize,
    pub edges: usize,
    pub dangling: usize,
    /// `None` when no symbol matches the focus.
    pub view: Option<ExploreView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExploreView {
    pub focus: Option<Node>,
    pub container: Option<Node>,
    pub callees: Vec<Node>,
    pub callers: Vec<Node>,
    pub importers: Vec<Node>,
    pub members: Vec<Node>,
    /// Dependents within the blast radius, and how many per repo.
    pub dependents: usize,
    pub by_repo: BTreeMap<String, usize>,
    pub budget: usize,
    /// The context pack: symbols to read with their estimated tokens.
    pub pack: Vec<(Node, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub symbols: usize,
    /// Repos in the index; `repos` holds those reported.
    pub indexed: usize,
    /// Asked-for repos the index does not have.
    pub unknown: Vec<String>,
    pub repos: Vec<RepoStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    pub repo: String,
    pub symbols: usize,
    pub edges: usize,
    pub enriched: usize,
    pub kinds: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preflight {
    pub markdown: String,
    /// A `must` constraint check fails in a changed file.
    pub blocking: bool,
}

/// Hops the blast radius follows, and the context pack's token budget.
pub const RIPPLE_DEPTH: usize = 4;

/// What an agent hook adds to the agent's context: records not yet shown this session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Hook {
    pub text: String,
    /// The records `text` shows, for the caller to remember as shown.
    pub ids: Vec<String>,
}
