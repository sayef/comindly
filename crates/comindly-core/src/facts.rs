//! Per-file syntactic facts the parser extracts and the resolver binds: imports, references
//! (as dotted paths) and type hints. Scope ids are rendered `GlobalSymbolId`s.

use serde::{Deserialize, Serialize};

use crate::model::Language;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileFacts {
    pub repo: String,
    pub path: String,
    pub language: Language,
    /// Rendered id of the file symbol.
    pub file_id: String,
    /// Names other files import this one by (`acme.common.http`, `@acme/ui-kit`).
    pub modules: Vec<String>,
    pub imports: Vec<Import>,
    pub refs: Vec<Ref>,
    pub types: Vec<TypeHint>,
    /// Compiler-grade occurrences from a SCIP index, when one was ingested.
    #[serde(default)]
    pub scip: Option<ScipFacts>,
    /// Service-boundary facts (routes, clients, queue/bucket/table operations).
    #[serde(default)]
    pub contracts: Vec<ContractFact>,
    /// Statically known values of module- and class-level names (for contract keys).
    #[serde(default)]
    pub values: Vec<ValueFact>,
    /// Parameter names of callables, in order (receiver excluded), for param-flow summaries.
    #[serde(default)]
    pub params: Vec<ParamFact>,
    /// Calls with statically readable arguments, for param-flow summaries.
    #[serde(default)]
    pub calls: Vec<CallFact>,
}

/// A queue (`sqs`), topic (`sns`) or bucket (`s3`) named in IaC; `logical` names are the
/// short name a module expands (`inference-tasks`), not the full resource name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfraRef {
    pub scheme: String,
    pub name: String,
    pub logical: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamFact {
    pub func: String,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFact {
    pub from: String,
    pub callee: Vec<String>,
    pub args: Args,
    pub line: u32,
}

/// A value an expression evaluates to, as far as static analysis can tell.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValueExpr {
    Lit(String),
    /// An f-string / template: literal parts and placeholder expressions.
    Template(Vec<TemplatePart>),
    /// A name or attribute path, resolved later (`QUEUE_URL`, `settings.bucket`).
    Name(Vec<String>),
    /// An environment variable read (`os.environ["X"]`, `os.getenv("X")`).
    Env(String),
    /// `a / b` (path join, `sep = "/"`) or `a + b` (concatenation, `sep = ""`).
    Join {
        sep: String,
        parts: Vec<ValueExpr>,
    },
    /// A call: `Path("/x")`, `str(x)`, `AttrDict(k=v)`; evaluated by known callee names.
    Call {
        func: Vec<String>,
        args: Vec<ValueExpr>,
        kwargs: Vec<(String, ValueExpr)>,
    },
    /// A literal mapping with string keys: `{"k": v}`.
    Record(Vec<(String, ValueExpr)>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplatePart {
    Lit(String),
    Expr(Option<Vec<String>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueFact {
    /// File id (module level) or class id (class attribute).
    pub scope: String,
    pub name: String,
    pub value: ValueExpr,
}

/// A call's arguments as values: positional, then `name=` keywords.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Args {
    pub positional: Vec<Option<ValueExpr>>,
    pub keywords: Vec<(String, ValueExpr)>,
    /// Positional arguments that are calls, by callee path (`Course(...)`).
    pub calls: Vec<(usize, Vec<String>)>,
}

impl Args {
    pub fn get(&self, keyword: &str, position: usize) -> Option<&ValueExpr> {
        self.keywords
            .iter()
            .find(|(k, _)| k == keyword)
            .map(|(_, v)| v)
            .or_else(|| self.positional.get(position).and_then(Option::as_ref))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractFact {
    /// `@router.get("/x")` on `handler`.
    Route {
        handler: String,
        receiver: Vec<String>,
        method: String,
        path: ValueExpr,
        line: u32,
        /// A declarative client method (Feign) consumes the route instead of serving it.
        #[serde(default)]
        client: bool,
    },
    /// `mux.HandleFunc("GET /x", h)` / `app.get("/x", h)` in `from`; `handler` names the
    /// function when it is a reference rather than an inline closure.
    Handle {
        from: String,
        /// The router or route group the handler is registered on.
        #[serde(default)]
        receiver: Vec<String>,
        method: String,
        path: ValueExpr,
        handler: Vec<String>,
        line: u32,
    },
    /// `target = Ctor(args)` where the constructor may be a router, app or client.
    Construct {
        scope: String,
        target: String,
        ctor: Vec<String>,
        args: Args,
    },
    /// A method or function call that may be a service operation.
    Op {
        from: String,
        receiver: Vec<String>,
        method: String,
        args: Args,
        line: u32,
    },
    /// Infrastructure as code declares (or subscribes) a queue, topic or bucket.
    Infra {
        declares: InfraRef,
        /// The topic this queue is subscribed to.
        #[serde(default)]
        subscribes: Option<InfraRef>,
    },
    /// `__tablename__ = "x"` on a model class.
    Table { class: String, name: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipFacts {
    /// (SCIP symbol, our symbol id) for definitions in this file.
    pub defs: Vec<(String, String)>,
    pub refs: Vec<ScipRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipRef {
    pub line: u32,
    pub name: String,
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    /// Python dotted module (without leading dots) or a TS module specifier.
    pub module: String,
    /// Python relative-import level (number of leading dots); 0 for absolute and TS.
    pub level: u32,
    pub items: Vec<ImportItem>,
    pub line: u32,
    /// Repo-relative file a non-relative TS specifier maps to via tsconfig `paths`/`baseUrl`.
    #[serde(default)]
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportItem {
    /// `from m import name as alias` / `import { name as alias } from "m"`.
    Name { name: String, alias: Option<String> },
    /// The module itself: `import a.b [as x]`, `import * as x from "m"`, `import "m"`.
    Module { alias: Option<String> },
    /// `from m import *` / `export * from "m"`.
    All,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ref {
    pub kind: RefKind,
    /// Scope the reference occurs in (callable, class for bases, or file).
    pub from: String,
    /// Dotted path of the referenced expression, e.g. `["self", "api", "get"]`.
    pub path: Vec<String>,
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefKind {
    Call,
    New,
    Inherits,
    Implements,
}

/// `name` has a type in `scope`: a parameter/variable, or `self.x`/`this.x` on a class scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeHint {
    pub scope: String,
    pub name: String,
    pub source: TypeSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeSource {
    /// Declared type path: `x: Client`.
    Annotation(Vec<String>),
    /// Assigned from a call or `new`: `x = Client(...)`.
    Construct(Vec<String>),
    /// Copied from another name in another scope: `self.api = api` (param of `__init__`).
    Alias { name: String, scope: String },
}
