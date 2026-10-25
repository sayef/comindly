//! Per-language syntax adapters. The generic walk in [`crate::parse`] asks each question of the
//! file's language; adding a language means implementing [`LangSpec`] for its grammar.

pub mod go;
pub mod java;
pub mod kotlin;
pub mod python;
pub mod rust;
pub mod spring;
pub mod typescript;

use tree_sitter::Node;

use crate::facts::{Args, Import, RefKind, TypeSource, ValueExpr};
use crate::model::{Language, SymbolKind};

/// What an assignment's left-hand side names.
pub enum Target {
    /// A local or module variable.
    Var(String),
    /// An attribute on the instance (`self.x` / `this.x`).
    Attr(String),
}

/// Service-boundary syntax found at a node; the walk fills in scope ids.
pub enum RawContract {
    Route {
        handler: String,
        receiver: Vec<String>,
        method: String,
        path: ValueExpr,
        client: bool,
    },
    Handle {
        receiver: Vec<String>,
        method: String,
        path: ValueExpr,
        handler: Vec<String>,
    },
    Construct {
        target: Target,
        ctor: Vec<String>,
        args: Args,
    },
    Op {
        receiver: Vec<String>,
        method: String,
        args: Args,
    },
    TableName(String),
}

pub trait LangSpec: Sync {
    /// Receiver name inside methods (`self`, `this`).
    fn receiver(&self) -> &'static str;

    /// A definition node: its kind and name. `in_class` is true directly inside a class body.
    fn def(&self, node: Node, src: &[u8], in_class: bool) -> Option<(SymbolKind, String)>;

    /// A base declared apart from the type (Rust `impl Trait for Type`): (type name, kind, base).
    fn owned_bases(&self, _node: Node, _src: &[u8]) -> Option<(String, RefKind, Vec<String>)> {
        None
    }

    /// Contract syntax at this node (routes, client construction, service operations).
    fn contract(&self, _node: Node, _src: &[u8]) -> Option<RawContract> {
        None
    }

    /// Every contract at this node (a router line can register several verbs).
    fn contracts(&self, node: Node, src: &[u8]) -> Vec<RawContract> {
        self.contract(node, src).into_iter().collect()
    }

    /// Parameter names of a callable in order, receiver excluded.
    fn param_names(&self, _def: Node, _src: &[u8]) -> Vec<String> {
        Vec::new()
    }

    /// A call with at least one statically readable argument: callee path and arguments.
    fn call(&self, _node: Node, _src: &[u8]) -> Option<(Vec<String>, Args)> {
        None
    }

    /// `name = <statically known value>` at module or class level.
    fn value(&self, _node: Node, _src: &[u8]) -> Option<(String, ValueExpr)> {
        None
    }

    /// The package a file declares (JVM); `None` derives it from the path instead.
    fn package(&self, _root: Node, _src: &[u8]) -> Option<String> {
        None
    }

    /// The type a definition declared outside it belongs to (Go method receivers).
    fn owner(&self, _def: Node, _src: &[u8]) -> Option<String> {
        None
    }

    /// A module-level variable binding's name (only asked at module scope).
    fn variable(&self, node: Node, src: &[u8]) -> Option<String>;

    /// `Some` for an import/re-export statement (not descended into), possibly empty.
    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>>;

    /// A call or construction and the dotted path of its callee.
    fn reference(&self, node: Node, src: &[u8]) -> Option<(RefKind, Vec<String>)>;

    /// An assignment (or field declaration) whose target type can be inferred.
    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)>;

    /// Base classes / implemented interfaces of a class-like definition.
    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)>;

    /// Declared return type of a callable.
    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>>;

    /// Typed parameters of a callable; `true` marks constructor parameter properties (TS).
    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)>;
}

pub fn spec(lang: &Language) -> Option<&'static dyn LangSpec> {
    match lang {
        Language::Python => Some(&python::Python),
        Language::TypeScript | Language::JavaScript => Some(&typescript::TypeScript),
        Language::Go => Some(&go::Go),
        Language::Java => Some(&java::Java),
        Language::Kotlin => Some(&kotlin::Kotlin),
        Language::Rust => Some(&rust::Rust),
        _ => None,
    }
}

pub(crate) fn text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    node.utf8_text(src).unwrap_or("")
}

pub(crate) fn field_text<'a>(node: Node, field: &str, src: &'a [u8]) -> Option<&'a str> {
    node.child_by_field_name(field).map(|n| text(n, src))
}

pub(crate) fn named_children(node: Node) -> Vec<Node> {
    let mut c = node.walk();
    node.named_children(&mut c).collect()
}

pub(crate) fn line(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}
