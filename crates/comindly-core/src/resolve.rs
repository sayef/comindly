//! Comindly resolve — bind per-file facts to definitions, within and across repos.
//!
//! A reference path (`self.api.get`, `events.publish`, `kit.formatDate`) is resolved segment by
//! segment through imports, re-exports, modules, classes (incl. bases) and typed variables.
//! Emits `Imports`, `Calls`, `Instantiates`, `Inherits` and `Implements` edges.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::facts::{FileFacts, Import, ImportItem, RefKind, TypeSource};
use crate::model::{Edge, EdgeKind, Language, Symbol, SymbolKind};
use crate::packages::strip_ts_ext;

mod contracts;
pub use contracts::{contract_id, http_path, CONTRACTS};

/// Name → (line, SCIP symbol) occurrences within one file.
type ScipOccs<'a> = HashMap<&'a str, Vec<(u32, &'a str)>>;

/// Guard against re-export / alias cycles.
const MAX_DEPTH: u32 = 12;

#[derive(Debug, Default)]
pub struct Resolved {
    pub edges: Vec<Edge>,
    /// Contract nodes (routes, queues, buckets, tables) to add to the symbol set.
    pub symbols: Vec<Symbol>,
    pub stats: ResolveStats,
    /// Top-level module → count of import items that didn't resolve.
    pub unresolved_modules: HashMap<String, usize>,
    /// Top-level module → a few `path:line module item` examples.
    pub unresolved_examples: HashMap<String, Vec<String>>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ResolveStats {
    pub resolved_imports: usize,
    /// Imports whose target is outside the corpus (mostly third-party).
    pub unresolved_imports: usize,
    /// Calls and instantiations bound through scopes, imports and types.
    pub resolved_calls: usize,
    pub unresolved_calls: usize,
    /// `Inherits` + `Implements`.
    pub type_edges: usize,
    /// References answered by SCIP rather than tree-sitter resolution.
    pub scip_answers: usize,
    /// Participant → contract edges (serves, consumes, publishes, …).
    pub contract_edges: usize,
    pub cross_repo_edges: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    File(String),
    /// A module or package by name (a Python namespace package may have no file).
    Module {
        fam: Family,
        name: String,
        repo: String,
    },
    Sym(String),
    Instance(String),
}

/// Resolve the merged corpus. Input edges pass through; new edges come from `facts`.
pub fn resolve(symbols: &[Symbol], edges: &[Edge], facts: &[FileFacts]) -> Resolved {
    let b = Binder::new(symbols, edges, facts);
    let mut out = Resolved {
        edges: edges.to_vec(),
        ..Default::default()
    };
    let mut new_edges: Vec<(String, String, EdgeKind, f32)> = Vec::new();

    // Bases first: method lookup walks them.
    for f in facts {
        for r in &f.refs {
            if !matches!(r.kind, RefKind::Inherits | RefKind::Implements) {
                continue;
            }
            let scope = b.parent.get(&r.from).cloned().unwrap_or(f.file_id.clone());
            let name = r.path.last().map_or("", String::as_str);
            let target = match b.scip(f, name, r.line) {
                Some(t) => {
                    out.stats.scip_answers += 1;
                    Some(t)
                }
                None => match b.resolve_path(&r.path, &scope, f, 0) {
                    Some(Value::Sym(t)) => Some(t),
                    _ => None,
                },
            };
            let Some(t) = target else {
                continue;
            };
            if !b.is_type(&t) {
                continue;
            }
            b.bases
                .borrow_mut()
                .entry(r.from.clone())
                .or_default()
                .push(t.clone());
            let kind = if r.kind == RefKind::Inherits {
                EdgeKind::Inherits
            } else {
                EdgeKind::Implements
            };
            new_edges.push((r.from.clone(), t, kind, 0.9));
            out.stats.type_edges += 1;
        }
    }

    for f in facts {
        for imp in &f.imports {
            let base = b.import_target(f, imp);
            for item in &imp.items {
                let target = match item {
                    ImportItem::Name { name, .. } => match b.scip(f, name, imp.line) {
                        Some(t) => {
                            out.stats.scip_answers += 1;
                            Some(Value::Sym(t))
                        }
                        None => base.as_ref().and_then(|v| b.member(v, name, 0)),
                    },
                    ImportItem::Module { .. } | ImportItem::All => base.clone(),
                };
                match target.and_then(|v| b.node_of(&v)) {
                    Some(t) if t != f.file_id => {
                        new_edges.push((f.file_id.clone(), t, EdgeKind::Imports, 0.95));
                        out.stats.resolved_imports += 1;
                    }
                    Some(_) => {}
                    None => {
                        out.stats.unresolved_imports += 1;
                        let top = top_module(f, imp);
                        let ex = out.unresolved_examples.entry(top.clone()).or_default();
                        if ex.len() < 3 {
                            ex.push(format!("{}:{} {} {:?}", f.path, imp.line, imp.module, item));
                        }
                        *out.unresolved_modules.entry(top).or_default() += 1;
                    }
                }
            }
        }

        for r in &f.refs {
            if !matches!(r.kind, RefKind::Call | RefKind::New) {
                continue;
            }
            let name = r.path.last().map_or("", String::as_str);
            let target = match b.scip(f, name, r.line) {
                Some(t) => {
                    out.stats.scip_answers += 1;
                    Some(t)
                }
                None => match b.resolve_path(&r.path, &r.from, f, 0) {
                    Some(Value::Sym(t)) => Some(t),
                    _ => None,
                },
            };
            let Some(t) = target else {
                out.stats.unresolved_calls += 1;
                continue;
            };
            let kind = if b.is_type(&t) {
                EdgeKind::Instantiates
            } else if b.is_callable(&t) {
                EdgeKind::Calls
            } else {
                out.stats.unresolved_calls += 1;
                continue;
            };
            out.stats.resolved_calls += 1;
            new_edges.push((r.from.clone(), t, kind, 0.9));
        }
    }

    let calls: Vec<(String, String)> = new_edges
        .iter()
        .filter(|(_, _, k, _)| *k == EdgeKind::Calls)
        .map(|(s, d, _, _)| (s.clone(), d.clone()))
        .collect();
    let linked = contracts::link(&b, facts, &calls);
    for (src, dst, kind) in linked.edges {
        // Sources are code symbols, or contracts themselves (a queue subscribed to a topic).
        let Some(src_id) = b.sym(&src).map(|s| s.id.clone()).or_else(|| {
            linked
                .symbols
                .iter()
                .find(|c| c.id.render() == src)
                .map(|c| c.id.clone())
        }) else {
            continue;
        };
        let dst_id = linked
            .symbols
            .iter()
            .find(|c| c.id.render() == dst)
            .map(|c| c.id.clone());
        if let Some(d) = dst_id {
            out.stats.contract_edges += 1;
            out.edges.push(Edge {
                src: src_id,
                dst: d,
                kind,
                confidence: 0.9,
                cross_repo: false,
            });
        }
    }
    out.symbols = linked.symbols;

    let mut seen: HashSet<(String, String, EdgeKind)> = out
        .edges
        .iter()
        .map(|e| (e.src.render(), e.dst.render(), e.kind.clone()))
        .collect();
    for (src, dst, kind, confidence) in new_edges {
        let (Some(s), Some(d)) = (b.sym(&src), b.sym(&dst)) else {
            continue;
        };
        if !seen.insert((src, dst, kind.clone())) {
            continue;
        }
        let cross_repo = s.repo != d.repo;
        if cross_repo {
            out.stats.cross_repo_edges += 1;
        }
        out.edges.push(Edge {
            src: s.id.clone(),
            dst: d.id.clone(),
            kind,
            confidence,
            cross_repo,
        });
    }
    out
}

/// Languages sharing module semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Family {
    Py,
    Js,
    Go,
    Jvm,
    Rust,
}

impl Family {
    /// Separator between a module and its submodules, where submodules exist.
    fn sub_sep(self) -> Option<&'static str> {
        match self {
            Family::Py => Some("."),
            Family::Rust => Some("::"),
            _ => None,
        }
    }
}

fn family(lang: &Language) -> Option<Family> {
    match lang {
        Language::Python => Some(Family::Py),
        Language::TypeScript | Language::JavaScript => Some(Family::Js),
        Language::Go => Some(Family::Go),
        Language::Java | Language::Kotlin => Some(Family::Jvm),
        Language::Rust => Some(Family::Rust),
        _ => None,
    }
}

struct Binder<'a> {
    syms: HashMap<String, &'a Symbol>,
    parent: HashMap<String, String>,
    /// Parent id → child name → child id.
    members: HashMap<String, HashMap<&'a str, String>>,
    facts: HashMap<&'a str, &'a FileFacts>,
    /// Module / package name → files (one per Python module, many per Go package).
    modules: HashMap<(Family, &'a str), Vec<&'a FileFacts>>,
    /// Every prefix of a Python/Rust module path (namespace packages, `mod` parents).
    packages: HashSet<(Family, String)>,
    by_path: HashMap<(&'a str, &'a str), &'a FileFacts>,
    types: HashMap<(&'a str, &'a str), &'a TypeSource>,
    /// Statically known values of module/class names (object literals act as namespaces).
    values: HashMap<(&'a str, &'a str), &'a crate::facts::ValueExpr>,
    bases: RefCell<HashMap<String, Vec<String>>>,
    /// SCIP symbol → our id, for every definition SCIP mapped.
    scip_defs: HashMap<&'a str, &'a str>,
    /// File id → name → (line, SCIP symbol) of reference occurrences.
    scip_refs: HashMap<&'a str, ScipOccs<'a>>,
}

impl<'a> Binder<'a> {
    fn new(symbols: &'a [Symbol], edges: &[Edge], facts: &'a [FileFacts]) -> Self {
        let syms: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.render(), s)).collect();
        let mut parent = HashMap::new();
        let mut members: HashMap<String, HashMap<&str, String>> = HashMap::new();
        for e in edges.iter().filter(|e| e.kind == EdgeKind::Contains) {
            let (p, c) = (e.src.render(), e.dst.render());
            if let Some(child) = syms.get(&c) {
                members
                    .entry(p.clone())
                    .or_default()
                    .insert(child.name.as_str(), c.clone());
            }
            parent.insert(c, p);
        }
        let mut b = Binder {
            syms,
            parent,
            members,
            facts: HashMap::new(),
            modules: HashMap::new(),
            packages: HashSet::new(),
            by_path: HashMap::new(),
            types: HashMap::new(),
            values: HashMap::new(),
            bases: RefCell::new(HashMap::new()),
            scip_defs: HashMap::new(),
            scip_refs: HashMap::new(),
        };
        for f in facts {
            b.facts.insert(f.file_id.as_str(), f);
            b.by_path.insert((f.repo.as_str(), f.path.as_str()), f);
            let Some(fam) = family(&f.language) else {
                continue;
            };
            for m in &f.modules {
                b.modules.entry((fam, m.as_str())).or_default().push(f);
                if let Some(sep) = fam.sub_sep() {
                    let mut prefix = String::new();
                    for part in m.split(sep) {
                        if !prefix.is_empty() {
                            prefix.push_str(sep);
                        }
                        prefix.push_str(part);
                        b.packages.insert((fam, prefix.clone()));
                    }
                }
            }
            for t in &f.types {
                b.types
                    .entry((t.scope.as_str(), t.name.as_str()))
                    .or_insert(&t.source);
            }
            for v in &f.values {
                b.values
                    .insert((v.scope.as_str(), v.name.as_str()), &v.value);
            }
            if let Some(sf) = &f.scip {
                for (sym, id) in &sf.defs {
                    b.scip_defs.insert(sym.as_str(), id.as_str());
                }
                let by_name = b.scip_refs.entry(f.file_id.as_str()).or_default();
                for r in &sf.refs {
                    by_name
                        .entry(r.name.as_str())
                        .or_default()
                        .push((r.line, r.symbol.as_str()));
                }
            }
        }
        b
    }

    /// SCIP's exact target for `name` referenced at `line`, when it maps into the corpus.
    /// Unknown targets defer to tree-sitter resolution (other repos may lack SCIP).
    fn scip(&self, f: &FileFacts, name: &str, line: u32) -> Option<String> {
        let occs = self.scip_refs.get(f.file_id.as_str())?.get(name)?;
        // A call spanning lines puts the callee name at or just after the statement start.
        let (_, sym) = occs
            .iter()
            .filter(|(l, _)| *l >= line && *l <= line + 3)
            .min_by_key(|(l, _)| *l)?;
        self.scip_defs.get(sym).map(|id| id.to_string())
    }

    fn sym(&self, id: &str) -> Option<&'a Symbol> {
        self.syms.get(id).copied()
    }

    fn is_type(&self, id: &str) -> bool {
        self.sym(id).is_some_and(|s| {
            matches!(
                s.kind,
                SymbolKind::Class
                    | SymbolKind::Interface
                    | SymbolKind::Struct
                    | SymbolKind::Trait
                    | SymbolKind::Enum
            )
        })
    }

    fn is_callable(&self, id: &str) -> bool {
        self.sym(id)
            .is_some_and(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
    }

    /// Files of a module: those in the importing repo, else those of the only repo defining it.
    fn module_files(&self, fam: Family, name: &str, repo: &str) -> Vec<&'a FileFacts> {
        let Some(cands) = self.modules.get(&(fam, name)) else {
            return Vec::new();
        };
        let local: Vec<_> = cands.iter().filter(|f| f.repo == repo).copied().collect();
        if !local.is_empty() {
            return local;
        }
        let first = &cands[0].repo;
        if cands.iter().all(|f| &f.repo == first) {
            cands.clone()
        } else {
            Vec::new()
        }
    }

    /// What an import statement's module refers to.
    fn import_target(&self, f: &FileFacts, imp: &Import) -> Option<Value> {
        let module = |fam, name: String| Value::Module {
            fam,
            name,
            repo: f.repo.clone(),
        };
        match family(&f.language)? {
            Family::Py => {
                if imp.level == 0 {
                    return Some(module(Family::Py, imp.module.clone()));
                }
                let own = f.modules.first()?;
                let mut parts: Vec<&str> = own.split('.').collect();
                let is_pkg = f.path.ends_with("__init__.py") || f.path.ends_with("__init__.pyi");
                let drop = imp.level as usize - usize::from(is_pkg);
                parts.truncate(parts.len().checked_sub(drop)?);
                if !imp.module.is_empty() {
                    parts.push(&imp.module);
                }
                Some(module(Family::Py, parts.join(".")))
            }
            fam @ (Family::Go | Family::Jvm) => Some(module(fam, imp.module.clone())),
            // Uniform paths: `use store::X` may name a child module rather than a crate.
            Family::Rust => {
                let head = imp.module.split("::").next().unwrap_or_default();
                let local = f
                    .modules
                    .first()
                    .filter(|own| {
                        self.packages
                            .contains(&(Family::Rust, format!("{own}::{head}")))
                    })
                    .map(|own| format!("{own}::{}", imp.module));
                Some(module(
                    Family::Rust,
                    local.unwrap_or_else(|| imp.module.clone()),
                ))
            }
            Family::Js if imp.file.is_some() => self
                .by_path
                .get(&(f.repo.as_str(), imp.file.as_deref()?))
                .map(|t| Value::File(t.file_id.clone())),
            Family::Js if imp.module.starts_with('.') => {
                let dir = f.path.rsplit_once('/').map_or("", |(d, _)| d);
                let base = normalize(dir, &imp.module)?;
                let stem = strip_ts_ext(&base);
                [
                    ".ts",
                    ".tsx",
                    ".d.ts",
                    ".js",
                    ".jsx",
                    ".mjs",
                    "/index.ts",
                    "/index.tsx",
                    "/index.js",
                ]
                .iter()
                .find_map(|e| {
                    let path = format!("{stem}{e}");
                    self.by_path
                        .get(&(f.repo.as_str(), path.as_str()))
                        .map(|t| Value::File(t.file_id.clone()))
                })
            }
            Family::Js => {
                let files = self.module_files(Family::Js, &imp.module, &f.repo);
                (files.len() == 1).then(|| Value::File(files[0].file_id.clone()))
            }
        }
    }

    /// The graph node a value denotes (file or symbol id); a package maps to its main file.
    fn node_of(&self, v: &Value) -> Option<String> {
        match v {
            Value::File(id) | Value::Sym(id) => Some(id.clone()),
            Value::Module { fam, name, repo } => {
                let files = self.module_files(*fam, name, repo);
                let last = name.rsplit(['/', '.']).next().unwrap_or(name);
                files
                    .iter()
                    .find(|f| {
                        f.path.rsplit('/').next().and_then(|n| n.split('.').next()) == Some(last)
                    })
                    .or_else(|| files.iter().min_by_key(|f| &f.path))
                    .map(|f| f.file_id.clone())
            }
            Value::Instance(_) => None,
        }
    }

    fn member(&self, v: &Value, name: &str, depth: u32) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        match v {
            Value::File(f) => self.module_member(f, name, true, depth + 1),
            Value::Module { fam, name: m, repo } => {
                // Python/JS/Rust modules re-export what they import; Go/JVM packages don't.
                let reexports = matches!(fam, Family::Py | Family::Js | Family::Rust);
                let files = self.module_files(*fam, m, repo);
                if let Some(x) = files
                    .iter()
                    .find_map(|f| self.module_member(&f.file_id, name, reexports, depth + 1))
                {
                    return Some(x);
                }
                // `a.b.Outer.Inner` (JVM) / `krate::Enum::Variant` (Rust): walk through the type.
                let sep = match fam {
                    Family::Jvm => Some("."),
                    Family::Rust => Some("::"),
                    _ => None,
                };
                if let (Some(sep), true) = (sep, files.is_empty()) {
                    let (pkg, outer) = m.rsplit_once(sep)?;
                    let outer = self.member(
                        &Value::Module {
                            fam: *fam,
                            name: pkg.to_string(),
                            repo: repo.clone(),
                        },
                        outer,
                        depth + 1,
                    )?;
                    return self.member(&outer, name, depth + 1);
                }
                let sub = format!("{m}{}{name}", fam.sub_sep()?);
                self.packages
                    .contains(&(*fam, sub.clone()))
                    .then(|| Value::Module {
                        fam: *fam,
                        name: sub,
                        repo: repo.clone(),
                    })
            }
            Value::Sym(id) if self.is_type(id) => self.class_member(id, name, depth + 1),
            // A module variable: an object literal's field, else a member of its inferred type.
            Value::Sym(id) => {
                let var = self
                    .sym(id)
                    .filter(|s| matches!(s.kind, SymbolKind::Variable | SymbolKind::Constant))?;
                let scope = self.parent.get(id)?;
                if let Some(crate::facts::ValueExpr::Record(fields)) =
                    self.values.get(&(scope.as_str(), var.name.as_str()))
                {
                    if let Some((_, v)) = fields.iter().find(|(k, _)| k == name) {
                        return match v {
                            crate::facts::ValueExpr::Name(p) => {
                                let f = self.file_facts_of(scope)?;
                                self.resolve_path(p, scope, f, depth + 1)
                            }
                            _ => None,
                        };
                    }
                }
                let src = self.types.get(&(scope.as_str(), var.name.as_str()))?;
                let inst = self.type_of(src, scope, depth + 1)?;
                self.member(&inst, name, depth + 1)
            }
            Value::Instance(c) => {
                for recv in ["self", "this"] {
                    let attr = format!("{recv}.{name}");
                    if let Some(src) = self.types.get(&(c.as_str(), attr.as_str())) {
                        if let Some(v) = self.type_of(src, c, depth + 1) {
                            return Some(v);
                        }
                        // A field holding a function reference (`readonly fetch = _fetch`).
                        if let TypeSource::Alias { name: n, scope: s } = src {
                            let f = self.file_facts_of(s)?;
                            if let Some(v @ Value::Sym(_)) = self.resolve_name(n, s, f, depth + 1) {
                                return Some(v);
                            }
                        }
                        return None;
                    }
                }
                self.class_member(c, name, depth + 1)
            }
        }
    }

    /// A name in a file's namespace: its definitions, then (optionally) what its imports bind.
    fn module_member(&self, file: &str, name: &str, imports: bool, depth: u32) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        if let Some(id) = self.members.get(file).and_then(|m| m.get(name)) {
            return Some(Value::Sym(id.clone()));
        }
        if !imports {
            return None;
        }
        let f = self.facts.get(file)?;
        for imp in f.imports.iter().rev() {
            for item in &imp.items {
                let hit = match item {
                    ImportItem::Name { name: n, alias } => {
                        if alias.as_deref().unwrap_or(n) != name {
                            continue;
                        }
                        self.import_target(f, imp)
                            .and_then(|t| self.member(&t, n, depth + 1))
                    }
                    ImportItem::Module { alias: Some(a) } if a == name => {
                        self.import_target(f, imp)
                    }
                    // `import a.b` binds `a`.
                    ImportItem::Module { alias: None }
                        if f.language == Language::Python
                            && imp.module.split('.').next() == Some(name) =>
                    {
                        Some(Value::Module {
                            fam: Family::Py,
                            name: name.to_string(),
                            repo: f.repo.clone(),
                        })
                    }
                    ImportItem::All => self
                        .import_target(f, imp)
                        .and_then(|t| self.member(&t, name, depth + 1)),
                    _ => continue,
                };
                if hit.is_some() {
                    return hit;
                }
            }
        }
        None
    }

    fn class_member(&self, class: &str, name: &str, depth: u32) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        if let Some(id) = self.members.get(class).and_then(|m| m.get(name)) {
            return Some(Value::Sym(id.clone()));
        }
        let bases = self.bases.borrow().get(class).cloned().unwrap_or_default();
        bases
            .iter()
            .find_map(|b| self.class_member(b, name, depth + 1))
    }

    /// The instance type a hint denotes, read in `scope`.
    fn type_of(&self, src: &TypeSource, scope: &str, depth: u32) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        match src {
            TypeSource::Annotation(p) => {
                let f = self.file_facts_of(scope)?;
                match self.resolve_path(p, scope, f, depth + 1)? {
                    Value::Sym(t) if self.is_type(&t) => Some(Value::Instance(t)),
                    _ => None,
                }
            }
            // `x = Cls(...)` is an instance; `x = make()` takes make's declared return type.
            TypeSource::Construct(p) => {
                let f = self.file_facts_of(scope)?;
                match self.resolve_path(p, scope, f, depth + 1)? {
                    Value::Sym(t) if self.is_type(&t) => Some(Value::Instance(t)),
                    Value::Sym(t) if self.is_callable(&t) => {
                        let ret = self.types.get(&(t.as_str(), crate::parse::RETURN))?;
                        self.type_of(ret, &t, depth + 1)
                    }
                    _ => None,
                }
            }
            TypeSource::Alias { name, scope: s } => {
                let f = self.file_facts_of(s)?;
                match self.resolve_name(name, s, f, depth + 1)? {
                    v @ Value::Instance(_) => Some(v),
                    _ => None,
                }
            }
        }
    }

    /// The file a scope is written in (a Go method's parent type may live elsewhere).
    fn file_facts_of(&self, id: &str) -> Option<&'a FileFacts> {
        if let Some(f) = self.facts.get(id) {
            return Some(f);
        }
        let s = self.sym(id)?;
        self.by_path
            .get(&(s.repo.0.as_str(), s.file_path.as_str()))
            .copied()
    }

    /// Scopes from `scope` outward to (and including) the file.
    fn scope_chain(&self, scope: &str) -> Vec<String> {
        let mut out = vec![scope.to_string()];
        let mut cur = scope;
        while let Some(p) = self.parent.get(cur) {
            out.push(p.clone());
            cur = p;
        }
        out
    }

    fn enclosing_class(&self, scope: &str) -> Option<String> {
        self.scope_chain(scope)
            .into_iter()
            .find(|s| self.is_type(s))
    }

    fn resolve_name(&self, head: &str, scope: &str, f: &FileFacts, depth: u32) -> Option<Value> {
        if head == "self" || head == "this" {
            return self.enclosing_class(scope).map(Value::Instance);
        }
        if f.language == Language::Rust {
            if let Some(v) = self.rust_head(head, scope, f) {
                return Some(v);
            }
        }
        for s in self.scope_chain(scope) {
            if let Some(src) = self.types.get(&(s.as_str(), head)) {
                if let Some(v) = self.type_of(src, &s, depth + 1) {
                    return Some(v);
                }
                // A local of unknown type shadows outer names; a module variable is its symbol.
                if s != f.file_id {
                    return None;
                }
            }
            // Functions nested in an enclosing callable (closures).
            if self.is_callable(&s) {
                if let Some(id) = self.members.get(&s).and_then(|m| m.get(head)) {
                    return Some(Value::Sym(id.clone()));
                }
            }
        }
        let fam = family(&f.language)?;
        // JVM: members and fields of the enclosing class (and its bases) need no `this.`.
        if fam == Family::Jvm {
            if let Some(c) = self.enclosing_class(scope) {
                if let Some(v) = self.member(&Value::Instance(c), head, depth + 1) {
                    return Some(v);
                }
            }
        }
        if let Some(v) = self.module_member(&f.file_id, head, true, depth + 1) {
            return Some(v);
        }
        // Rust: a child module (`mod util;`) or another crate named directly.
        if fam == Family::Rust {
            let own = f.modules.first()?;
            for cand in [format!("{own}::{head}"), head.to_string()] {
                if self.packages.contains(&(Family::Rust, cand.clone())) {
                    return Some(Value::Module {
                        fam,
                        name: cand,
                        repo: f.repo.clone(),
                    });
                }
            }
            return None;
        }
        // Go/JVM: the whole package shares one namespace across files.
        if !matches!(fam, Family::Go | Family::Jvm) {
            return None;
        }
        let own = f.modules.first()?;
        self.module_files(fam, own, &f.repo)
            .iter()
            .filter(|s| s.file_id != f.file_id)
            .find_map(|s| self.module_member(&s.file_id, head, false, depth + 1))
    }

    /// Rust path heads: `Self` (the impl type), `crate`, `<self>` and `super` (modules).
    fn rust_head(&self, head: &str, scope: &str, f: &FileFacts) -> Option<Value> {
        let own = f.modules.first()?;
        let name = match head {
            "Self" => return self.enclosing_class(scope).map(Value::Sym),
            "crate" => own.split("::").next()?.to_string(),
            "<self>" => own.clone(),
            "super" => own.rsplit_once("::")?.0.to_string(),
            _ => return None,
        };
        Some(Value::Module {
            fam: Family::Rust,
            name,
            repo: f.repo.clone(),
        })
    }

    fn resolve_path(
        &self,
        path: &[String],
        scope: &str,
        f: &FileFacts,
        depth: u32,
    ) -> Option<Value> {
        let (head, rest) = path.split_first()?;
        let mut v = self.resolve_name(head, scope, f, depth)?;
        for seg in rest {
            v = self.member(&v, seg, depth)?;
        }
        Some(v)
    }
}

/// The package an import names: `fastapi`, `@scope/pkg`, or `./rel` for relative imports.
fn top_module(f: &FileFacts, imp: &Import) -> String {
    if imp.level > 0 || imp.module.starts_with('.') {
        return format!("(relative in {})", f.repo);
    }
    if matches!(f.language, Language::Java | Language::Kotlin) {
        return imp
            .module
            .splitn(4, '.')
            .take(3)
            .collect::<Vec<_>>()
            .join(".");
    }
    // Go: host/org/repo identifies the module.
    if f.language == Language::Go
        && imp
            .module
            .split('/')
            .next()
            .is_some_and(|h| h.contains('.'))
    {
        return imp
            .module
            .splitn(4, '/')
            .take(3)
            .collect::<Vec<_>>()
            .join("/");
    }
    let mut parts = imp.module.split(['.', '/']);
    let first = parts.next().unwrap_or_default();
    match (first.starts_with('@'), parts.next()) {
        (true, Some(second)) => format!("{first}/{second}"),
        _ => first.to_string(),
    }
}

/// Join a relative module specifier onto a directory, resolving `.` and `..`.
fn normalize(dir: &str, spec: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in spec.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_relative_specifiers() {
        assert_eq!(normalize("src/a", "./b").as_deref(), Some("src/a/b"));
        assert_eq!(normalize("src/a", "../c/d").as_deref(), Some("src/c/d"));
        assert_eq!(normalize("", "./x").as_deref(), Some("x"));
        assert_eq!(normalize("", "../x"), None);
    }
}
