//! Comindly core domain model.
//!
//! Storage-agnostic types shared by every crate: global symbol identity (SCIP scheme),
//! symbols, edges, languages, and source ranges. No heavy dependencies — this crate is
//! the stable contract the rest of the workspace builds on.

use serde::{Deserialize, Serialize};

/// Languages we can parse. Backed by tree-sitter grammars (polyglot from day 1).
///
/// `Other` keeps the enum open so an unknown extension degrades gracefully instead of
/// failing indexing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Python,
    TypeScript,
    JavaScript,
    Go,
    Rust,
    Java,
    Kotlin,
    Ruby,
    CSharp,
    Cpp,
    C,
    Php,
    Scala,
    Swift,
    Other(String),
}

/// What a symbol *is*. Superset of the Python model, widened for polyglot codebases.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    File,
    Module,
    Namespace,
    Class,
    Interface,
    Trait,
    Enum,
    Struct,
    Function,
    Method,
    Field,
    Variable,
    Constant,
    Import,
    TypeAlias,
    /// A detected multi-step execution flow (comindly-specific higher-order node).
    Process,
    /// A cross-service contract: an HTTP route, queue, topic, bucket or table.
    Contract,
}

/// Edge kinds between symbols. `cross_repo` on [`Edge`] marks whether an edge spans repos.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Imports,
    Calls,
    Instantiates,
    Inherits,
    Implements,
    References,
    Defines,
    Uses,
    ParticipatesIn,
    /// Contract roles: provider side (serves/publishes/writes/defines) and consumer side.
    Serves,
    Consumes,
    Publishes,
    Subscribes,
    Reads,
    Writes,
}

impl EdgeKind {
    /// Stable lowercase label for user- and agent-facing output (not Rust `Debug`).
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Contains => "contains",
            EdgeKind::Imports => "imports",
            EdgeKind::Calls => "calls",
            EdgeKind::Instantiates => "instantiates",
            EdgeKind::Inherits => "inherits",
            EdgeKind::Implements => "implements",
            EdgeKind::References => "references",
            EdgeKind::Defines => "defines",
            EdgeKind::Uses => "uses",
            EdgeKind::ParticipatesIn => "participates_in",
            EdgeKind::Serves => "serves",
            EdgeKind::Consumes => "consumes",
            EdgeKind::Publishes => "publishes",
            EdgeKind::Subscribes => "subscribes",
            EdgeKind::Reads => "reads",
            EdgeKind::Writes => "writes",
        }
    }
}

impl std::fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Identifier for a repository within the federated index.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoId(pub String);

/// A git commit SHA — indexes are versioned per commit in S3.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Commit(pub String);

/// Globally-unique symbol identity following the SCIP symbol scheme, so identity is stable
/// and unique **across repos** — the primitive that makes cross-repo `ripple` possible.
///
/// Rendered form: `<scheme> <package_manager> <package> <version> <descriptor>`
/// e.g. `scip-python pip acme 1.4.0 acme/foo/bar().`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GlobalSymbolId {
    /// Indexer scheme, e.g. `scip-python`, `scip-typescript`, `comindly-treesitter`.
    pub scheme: String,
    /// Package manager, e.g. `pip`, `npm`, `cargo`, `.` when local/unmanaged.
    pub package_manager: String,
    /// Package name, e.g. `acme`.
    pub package: String,
    /// Package version, `.` when unknown.
    pub version: String,
    /// SCIP descriptor path, e.g. `acme/foo/bar().`.
    pub descriptor: String,
}

impl GlobalSymbolId {
    /// Render the canonical space-delimited SCIP symbol string.
    pub fn render(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.scheme, self.package_manager, self.package, self.version, self.descriptor
        )
    }

    /// True when this id refers to a package-managed (potentially cross-repo) symbol
    /// rather than a purely local one.
    pub fn is_package_managed(&self) -> bool {
        self.package_manager != "."
    }
}

/// A byte/line/column source position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// A half-open source range `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// A code entity extracted from a source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: GlobalSymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub language: Language,
    pub repo: RepoId,
    /// Repo-relative path.
    pub file_path: String,
    pub range: Range,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    #[serde(default)]
    pub fingerprint: Option<Fingerprint>,
}

/// Normalized AST hashes: signature, body, and body with identifiers alpha-renamed (rename-stable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint {
    pub sig: u64,
    pub body: u64,
    pub shape: u64,
}

/// A directed relationship between two symbols. Edges may cross repo boundaries once the
/// link-resolver has bound package-managed references to their definitions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub src: GlobalSymbolId,
    pub dst: GlobalSymbolId,
    pub kind: EdgeKind,
    /// Resolver confidence in `[0.0, 1.0]` (syntactic tree-sitter edges are < 1.0).
    pub confidence: f32,
    /// Whether `src` and `dst` live in different repos.
    pub cross_repo: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scip_id_renders_and_detects_managed() {
        let id = GlobalSymbolId {
            scheme: "scip-python".into(),
            package_manager: "pip".into(),
            package: "acme".into(),
            version: "1.4.0".into(),
            descriptor: "acme/foo/bar().".into(),
        };
        assert_eq!(id.render(), "scip-python pip acme 1.4.0 acme/foo/bar().");
        assert!(id.is_package_managed());

        let local = GlobalSymbolId {
            package_manager: ".".into(),
            ..id
        };
        assert!(!local.is_package_managed());
    }
}
