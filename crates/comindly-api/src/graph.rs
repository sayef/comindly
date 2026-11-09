use crate::model::EdgeKind;

/// A node as returned to callers — flat, printable, agent-friendly.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    /// `path:line` — jump straight here.
    pub location: String,
    pub signature: Option<String>,
}

/// A step of a flow: the node, how it was reached and from which node.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FlowStep {
    pub node: Node,
    pub depth: usize,
    pub via: String,
    pub from: String,
}

/// A node plus how far it sits from the focus (BFS depth) and via which relation.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Hop {
    pub node: Node,
    pub depth: usize,
    pub via: EdgeKind,
}

/// The 360° view of one symbol.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Zoom {
    pub focus: Option<Node>,
    pub container: Option<Node>,
    pub members: Vec<Node>,
    pub callers: Vec<Node>,
    pub callees: Vec<Node>,
    pub importers: Vec<Node>,
    /// Contracts this symbol takes part in, with the other side of each.
    pub contracts: Vec<ContractRole>,
    /// When the focus is itself a contract: who provides and who consumes it.
    pub providers: Vec<Node>,
    pub consumers: Vec<Node>,
}

/// A symbol's role in a contract (`serves http:GET /x`) and the participants on the other side.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContractRole {
    pub role: EdgeKind,
    pub contract: Node,
    pub counterparts: Vec<Node>,
}

/// A contract with everyone on each side.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContractView {
    pub contract: Node,
    pub providers: Vec<Node>,
    pub consumers: Vec<Node>,
}

/// The repo-like owner of contract nodes.
pub const CONTRACTS: &str = "_contracts";
