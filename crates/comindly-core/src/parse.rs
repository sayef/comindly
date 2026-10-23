//! Comindly parse — tree-sitter extraction of symbols, the `Contains` hierarchy and per-file facts.
//!
//! The walk is language-agnostic; [`crate::lang`] answers the syntax questions per grammar.
//! Imports, references and type hints go into [`FileFacts`], which [`crate::resolve`] binds.

use std::collections::HashMap;
use std::path::{Path, MAIN_SEPARATOR};

use crate::facts::{ContractFact, FileFacts, Ref, TypeHint, TypeSource, ValueFact};
use crate::lang::{self, LangSpec, RawContract, Target};
use crate::model::{
    Edge, EdgeKind, Fingerprint, GlobalSymbolId, Language, Position, Range, RepoId, Symbol,
    SymbolKind,
};
use crate::packages::RepoPackages;
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;
use tree_sitter::{Node, Parser};

const SCHEME: &str = "comindly-treesitter";

/// Type-hint name for a callable's declared return type.
pub const RETURN: &str = "<return>";

/// Symbols, structural edges and per-file facts extracted from a repo (or some files of it).
#[derive(Debug, Default)]
pub struct ParseOutput {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<Edge>,
    pub facts: Vec<FileFacts>,
}

impl ParseOutput {
    fn merge(&mut self, other: ParseOutput) {
        self.symbols.extend(other.symbols);
        self.edges.extend(other.edges);
        self.facts.extend(other.facts);
    }

    /// Collapse definitions sharing a SCIP id (e.g. `@overload` stubs), keeping the last one.
    fn dedup(&mut self) {
        use std::collections::HashSet;

        let mut last_idx: HashMap<String, usize> = HashMap::new();
        for (i, s) in self.symbols.iter().enumerate() {
            last_idx.insert(s.id.render(), i);
        }
        if last_idx.len() != self.symbols.len() {
            let keep: HashSet<usize> = last_idx.into_values().collect();
            let mut i = 0;
            self.symbols.retain(|_| {
                let k = keep.contains(&i);
                i += 1;
                k
            });
        }

        let mut seen: HashSet<(String, String, String)> = HashSet::new();
        self.edges
            .retain(|e| seen.insert((e.src.render(), e.dst.render(), format!("{:?}", e.kind))));
    }
}

/// Detect language from file extension. Returns `None` for files we don't parse.
fn detect(path: &Path) -> Option<Language> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("py" | "pyi") => Some(Language::Python),
        Some("ts" | "tsx") => Some(Language::TypeScript),
        Some("js" | "jsx" | "mjs" | "cjs") => Some(Language::JavaScript),
        Some("go") => Some(Language::Go),
        Some("java") => Some(Language::Java),
        Some("kt" | "kts") => Some(Language::Kotlin),
        Some("rs") => Some(Language::Rust),
        _ => None,
    }
}

fn tree_sitter_language(lang: &Language, is_tsx: bool) -> Option<tree_sitter::Language> {
    match lang {
        Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
        Language::TypeScript | Language::JavaScript if is_tsx => {
            Some(tree_sitter_typescript::LANGUAGE_TSX.into())
        }
        Language::TypeScript | Language::JavaScript => {
            Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        }
        Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
        Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
        Language::Kotlin => Some(tree_sitter_kotlin_ng::LANGUAGE.into()),
        Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
        _ => None,
    }
}

/// Parse every supported source file under `root`, in parallel, respecting `.gitignore`.
pub fn parse_repo(root: &Path, repo_name: &str) -> Result<ParseOutput> {
    let files: Vec<String> = WalkBuilder::new(root)
        .hidden(false)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter(|e| detect(e.path()).is_some())
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace(MAIN_SEPARATOR, "/"))
        })
        .collect();
    let mut out = parse_files(root, repo_name, &files);
    out.facts.extend(crate::configvals::scan(root, repo_name));
    for (s, f) in crate::infra::scan(root, repo_name) {
        out.symbols.push(s);
        out.facts.push(f);
    }
    Ok(out)
}

/// Parse a specific set of repo-relative files (for incremental indexing). Unsupported or
/// missing files are skipped.
pub fn parse_files(root: &Path, repo_name: &str, rel_paths: &[String]) -> ParseOutput {
    let packages = RepoPackages::discover(root);
    let mut out = rel_paths
        .par_iter()
        .filter_map(|rel| {
            let full = root.join(rel);
            full.is_file()
                .then(|| {
                    parse_file_with(&full, repo_name, root, &packages)
                        .ok()
                        .flatten()
                })
                .flatten()
        })
        .reduce(ParseOutput::default, |mut acc, o| {
            acc.merge(o);
            acc
        });
    out.dedup();
    // Config files only reach here on incremental runs (a full parse scans them separately).
    out.facts.extend(
        rel_paths
            .iter()
            .filter(|r| crate::configvals::is_config(r) && detect(&root.join(r)).is_none())
            .filter_map(|r| crate::configvals::file(root, repo_name, r)),
    );
    let tf: Vec<String> = rel_paths
        .iter()
        .filter(|r| crate::infra::is_tf(r) && root.join(r).is_file())
        .cloned()
        .collect();
    if !tf.is_empty() {
        for (s, f) in crate::infra::files(root, repo_name, &tf) {
            out.symbols.push(s);
            out.facts.push(f);
        }
    }
    out
}

/// Parse a single file. `Ok(None)` means "unsupported language", not an error.
pub fn parse_file(path: &Path, repo_name: &str, root: &Path) -> Result<Option<ParseOutput>> {
    parse_file_with(path, repo_name, root, &RepoPackages::discover(root))
}

fn parse_file_with(
    path: &Path,
    repo_name: &str,
    root: &Path,
    packages: &RepoPackages,
) -> Result<Option<ParseOutput>> {
    let Some(lang) = detect(path) else {
        return Ok(None);
    };
    let is_tsx = path.extension().and_then(|e| e.to_str()) == Some("tsx");
    let (Some(ts_lang), Some(spec)) = (tree_sitter_language(&lang, is_tsx), lang::spec(&lang))
    else {
        return Ok(None);
    };

    let source = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel_str = rel.to_string_lossy().replace(MAIN_SEPARATOR, "/");

    let mut parser = Parser::new();
    parser
        .set_language(&ts_lang)
        .context("set tree-sitter language")?;
    let Some(tree) = parser.parse(&source, None) else {
        return Ok(None);
    };

    let repo = RepoId(repo_name.to_string());
    let module = module_path(&rel_str);
    let file_id = id(repo_name, &module);
    let mut out = ParseOutput {
        symbols: vec![Symbol {
            id: file_id.clone(),
            name: rel_str.clone(),
            kind: SymbolKind::File,
            language: lang.clone(),
            repo: repo.clone(),
            file_path: rel_str.clone(),
            range: node_range(&tree.root_node()),
            signature: None,
            docstring: None,
            fingerprint: Some(fingerprint(tree.root_node(), &source, &lang)),
        }],
        edges: Vec::new(),
        facts: vec![FileFacts {
            repo: repo_name.to_string(),
            path: rel_str.clone(),
            language: lang.clone(),
            file_id: file_id.render(),
            modules: spec
                .package(tree.root_node(), &source)
                .map(|p| vec![p])
                .unwrap_or_else(|| packages.module_names(&rel_str, &lang)),
            imports: Vec::new(),
            refs: Vec::new(),
            types: Vec::new(),
            scip: None,
            contracts: Vec::new(),
            params: Vec::new(),
            calls: Vec::new(),
            values: Vec::new(),
        }],
    };

    let ctx = Ctx {
        source: &source,
        packages,
        lang: &lang,
        spec,
        repo: &repo,
        rel_str: &rel_str,
        repo_name,
    };
    // Go declarations live in the package (directory), not the file.
    let core = if lang == Language::Go {
        rel_str.rsplit_once('/').map_or("", |(d, _)| d).to_string()
    } else {
        module
    };
    let scope = Scope {
        core,
        id: file_id.clone(),
        callable: file_id,
        class: None,
        in_class: false,
        module_level: true,
    };
    walk(tree.root_node(), &scope, &ctx, &mut out);
    next_routes(&rel_str, &mut out);
    Ok(Some(out))
}

struct Ctx<'a> {
    source: &'a [u8],
    packages: &'a RepoPackages,
    lang: &'a Language,
    spec: &'static dyn LangSpec,
    repo: &'a RepoId,
    rel_str: &'a str,
    repo_name: &'a str,
}

#[derive(Clone)]
struct Scope {
    /// Descriptor path without the kind suffix, e.g. `acme/foo/MyClass`.
    core: String,
    /// The `Contains` parent.
    id: GlobalSymbolId,
    /// Nearest enclosing function/method (or the file): where references and locals live.
    callable: GlobalSymbolId,
    /// Nearest enclosing class: what `self`/`this` refers to.
    class: Option<GlobalSymbolId>,
    /// Directly inside a class body.
    in_class: bool,
    /// At module level (file or inline module), not inside a callable or class.
    module_level: bool,
}

fn walk(node: Node, scope: &Scope, ctx: &Ctx, out: &mut ParseOutput) {
    // Before defs: annotated definitions (Spring) declare contracts in their enclosing scope.
    for raw in ctx.spec.contracts(node, ctx.source) {
        contract(raw, node, scope, ctx, out);
    }
    if let Some((kind, name)) = ctx.spec.def(node, ctx.source, scope.in_class) {
        define(node, kind, &name, scope, ctx, out);
        return;
    }

    if let Some(mut imports) = ctx.spec.import(node, ctx.source) {
        for imp in imports.iter_mut().filter(|i| {
            matches!(ctx.lang, Language::TypeScript | Language::JavaScript)
                && !i.module.starts_with('.')
        }) {
            imp.file = ctx.packages.ts_alias(ctx.rel_str, &imp.module);
        }
        if *ctx.lang == Language::Rust {
            let own = facts(out).modules.first().cloned();
            for imp in &mut imports {
                imp.module = rust_absolute(&imp.module, own.as_deref());
            }
        }
        facts(out).imports.extend(imports);
        return; // imported names are not references
    }

    if let Some((ty, kind, path)) = ctx.spec.owned_bases(node, ctx.source) {
        let from = id(ctx.repo_name, &format!("{}#", join_core(&scope.core, &ty)));
        facts(out).refs.push(Ref {
            kind,
            from: from.render(),
            path,
            line: lang::line(node),
        });
    }

    if let Some((callee, args)) = ctx.spec.call(node, ctx.source) {
        facts(out).calls.push(crate::facts::CallFact {
            from: scope.callable.render(),
            callee,
            args,
            line: lang::line(node),
        });
    }

    if let Some((kind, path)) = ctx.spec.reference(node, ctx.source) {
        facts(out).refs.push(Ref {
            kind,
            from: scope.callable.render(),
            path,
            line: lang::line(node),
        });
    }

    let module_level = scope.module_level;
    if module_level {
        if let Some(name) = ctx.spec.variable(node, ctx.source) {
            variable(node, &name, scope, ctx, out);
        }
    }
    {
        if let Some((name, value)) = ctx.spec.value(node, ctx.source) {
            // `self.x = v` / `this.x = v` describe the class, wherever they are assigned.
            let attr = ["self.", "this."]
                .iter()
                .find_map(|r| name.strip_prefix(r))
                .map(str::to_string);
            let owner_name = match (attr, &scope.class) {
                (Some(a), Some(c)) => Some((c.clone(), a)),
                (Some(_), None) => None,
                (None, _) if scope.in_class => Some((scope.id.clone(), name)),
                (None, _) => Some((scope.callable.clone(), name)),
            };
            if let Some((owner, name)) = owner_name {
                facts(out).values.push(ValueFact {
                    scope: owner.render(),
                    name,
                    value,
                });
            }
        }
    }
    if let Some((target, source)) = ctx.spec.assignment(node, ctx.source) {
        type_hint(target, source, scope, ctx, out);
    }

    for c in children(node) {
        walk(c, scope, ctx, out);
    }
}

fn define(
    node: Node,
    kind: SymbolKind,
    name: &str,
    scope: &Scope,
    ctx: &Ctx,
    out: &mut ParseOutput,
) {
    // A Go method belongs to its receiver type, wherever that type is declared.
    let owner = ctx.spec.owner(node, ctx.source).map(|o| {
        let core = join_core(&scope.core, &o);
        (id(ctx.repo_name, &format!("{core}#")), core)
    });
    let (parent, parent_core, parent_class) = match &owner {
        Some((oid, ocore)) => (oid.clone(), ocore.as_str(), Some(oid.clone())),
        None => (scope.id.clone(), scope.core.as_str(), scope.class.clone()),
    };
    let core = join_core(parent_core, name);
    let sym_id = id(ctx.repo_name, &format!("{}{}", core, suffix(&kind)));
    out.symbols.push(Symbol {
        id: sym_id.clone(),
        name: name.to_string(),
        kind: kind.clone(),
        language: ctx.lang.clone(),
        repo: ctx.repo.clone(),
        file_path: ctx.rel_str.to_string(),
        range: node_range(&node),
        signature: Some(first_line(node, ctx.source)),
        docstring: None,
        fingerprint: Some(fingerprint(node, ctx.source, ctx.lang)),
    });
    out.edges
        .push(edge(&parent, &sym_id, EdgeKind::Contains, 1.0));

    let callable = is_callable(&kind);
    let class_like = matches!(
        kind,
        SymbolKind::Class
            | SymbolKind::Interface
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Enum
    );
    let rendered = sym_id.render();
    let line = lang::line(node);
    for (kind, path) in ctx.spec.bases(node, ctx.source) {
        facts(out).refs.push(Ref {
            kind,
            from: rendered.clone(),
            path,
            line,
        });
    }
    if callable {
        let names = ctx.spec.param_names(node, ctx.source);
        if !names.is_empty() {
            facts(out).params.push(crate::facts::ParamFact {
                func: rendered.clone(),
                names,
            });
        }
        if let Some(ty) = ctx.spec.returns(node, ctx.source) {
            facts(out).types.push(TypeHint {
                scope: rendered.clone(),
                name: RETURN.into(),
                source: TypeSource::Annotation(ty),
            });
        }
        for (param, ty, property) in ctx.spec.params(node, ctx.source) {
            let f = facts(out);
            if property {
                if let Some(class) = &parent_class {
                    f.types.push(TypeHint {
                        scope: class.render(),
                        name: format!("{}.{param}", ctx.spec.receiver()),
                        source: TypeSource::Annotation(ty.clone()),
                    });
                }
            }
            f.types.push(TypeHint {
                scope: rendered.clone(),
                name: param,
                source: TypeSource::Annotation(ty),
            });
        }
    }

    let child = Scope {
        core,
        id: sym_id.clone(),
        callable: if callable {
            sym_id.clone()
        } else {
            scope.callable.clone()
        },
        class: if class_like {
            Some(sym_id)
        } else {
            parent_class
        },
        in_class: class_like,
        module_level: kind == SymbolKind::Module,
    };
    for c in children(node) {
        walk(c, &child, ctx, out);
    }
}

/// A module-level binding: `Constant` when SCREAMING_CASE, else `Variable`.
fn variable(node: Node, name: &str, scope: &Scope, ctx: &Ctx, out: &mut ParseOutput) {
    let kind = if name.chars().any(|c| c.is_ascii_uppercase())
        && !name.chars().any(|c| c.is_ascii_lowercase())
    {
        SymbolKind::Constant
    } else {
        SymbolKind::Variable
    };
    let sym_id = id(
        ctx.repo_name,
        &format!("{}{}", join_core(&scope.core, name), suffix(&kind)),
    );
    out.symbols.push(Symbol {
        id: sym_id.clone(),
        name: name.to_string(),
        kind,
        language: ctx.lang.clone(),
        repo: ctx.repo.clone(),
        file_path: ctx.rel_str.to_string(),
        range: node_range(&node),
        signature: Some(first_line(node, ctx.source)),
        docstring: None,
        fingerprint: Some(fingerprint(node, ctx.source, ctx.lang)),
    });
    out.edges
        .push(edge(&scope.id, &sym_id, EdgeKind::Contains, 1.0));
}

fn contract(raw: RawContract, node: Node, scope: &Scope, ctx: &Ctx, out: &mut ParseOutput) {
    let line = lang::line(node);
    let fact = match raw {
        RawContract::Route {
            handler,
            receiver,
            method,
            path,
            client,
        } => ContractFact::Route {
            handler: id(
                ctx.repo_name,
                &format!(
                    "{}{}",
                    join_core(&scope.core, &handler),
                    suffix(&SymbolKind::Function)
                ),
            )
            .render(),
            receiver,
            method,
            path,
            line,
            client,
        },
        RawContract::Handle {
            receiver,
            method,
            path,
            handler,
        } => ContractFact::Handle {
            from: scope.callable.render(),
            receiver,
            method,
            path,
            handler,
            line,
        },
        RawContract::Construct { target, ctor, args } => {
            let (owner, name) = match target {
                Target::Var(n) => (scope.callable.render(), n),
                Target::Attr(n) => match &scope.class {
                    Some(c) => (c.render(), format!("{}.{n}", ctx.spec.receiver())),
                    None => return,
                },
            };
            ContractFact::Construct {
                scope: owner,
                target: name,
                ctor,
                args,
            }
        }
        RawContract::Op {
            receiver,
            method,
            args,
        } => ContractFact::Op {
            from: scope.callable.render(),
            receiver,
            method,
            args,
            line,
        },
        RawContract::TableName(name) => match &scope.class {
            Some(c) => ContractFact::Table {
                class: c.render(),
                name,
            },
            None => return,
        },
    };
    facts(out).contracts.push(fact);
}

fn type_hint(target: Target, source: TypeSource, scope: &Scope, ctx: &Ctx, out: &mut ParseOutput) {
    // An alias is read in the scope where the assignment happens.
    let source = match source {
        TypeSource::Alias { name, .. } => TypeSource::Alias {
            name,
            scope: scope.callable.render(),
        },
        s => s,
    };
    let hint = match target {
        Target::Var(name) => TypeHint {
            scope: scope.callable.render(),
            name,
            source,
        },
        Target::Attr(name) => {
            let Some(class) = &scope.class else {
                return;
            };
            TypeHint {
                scope: class.render(),
                name: format!("{}.{name}", ctx.spec.receiver()),
                source,
            }
        }
    };
    facts(out).types.push(hint);
}

/// Next.js app-router handlers: `route.ts` exporting `GET`/`POST`/… serve the file's path.
fn next_routes(rel: &str, out: &mut ParseOutput) {
    let Some(path) = crate::lang::typescript::next_route_path(rel) else {
        return;
    };
    let handlers: Vec<(String, String, u32)> = out
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .filter(|s| {
            matches!(
                s.name.as_str(),
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
            )
        })
        .map(|s| (s.id.render(), s.name.clone(), s.range.start.line))
        .collect();
    for (handler, method, line) in handlers {
        facts(out).contracts.push(ContractFact::Route {
            handler,
            receiver: Vec::new(),
            method,
            path: crate::facts::ValueExpr::Lit(path.clone()),
            line,
            client: false,
        });
    }
}

/// The single file's facts (a fresh `ParseOutput` from `parse_file_with` holds exactly one).
fn facts(out: &mut ParseOutput) -> &mut FileFacts {
    out.facts.last_mut().expect("file facts initialised")
}

// ---- helpers ------------------------------------------------------------------------

/// Hash a definition's leaf tokens, skipping comments, Python docstrings and whitespace.
fn fingerprint(node: Node, src: &[u8], lang: &Language) -> Fingerprint {
    let body_start = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    let (mut sig, mut body, mut shape) = (
        blake3::Hasher::new(),
        blake3::Hasher::new(),
        blake3::Hasher::new(),
    );
    let mut renames: HashMap<&[u8], u32> = HashMap::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind().contains("comment") || is_docstring(n, lang) {
            continue;
        }
        if n.child_count() > 0 {
            let mut cur = n.walk();
            let kids: Vec<Node> = n.children(&mut cur).collect();
            stack.extend(kids.into_iter().rev());
            continue;
        }
        let text = &src[n.start_byte()..n.end_byte()];
        for h in [&mut body, &mut shape] {
            h.update(n.kind().as_bytes());
            h.update(b"\0");
        }
        body.update(text);
        body.update(b"\0");
        if n.start_byte() < body_start {
            sig.update(n.kind().as_bytes());
            sig.update(b"\0");
            sig.update(text);
            sig.update(b"\0");
        }
        if n.kind().ends_with("identifier") {
            let next = renames.len() as u32;
            shape.update(&renames.entry(text).or_insert(next).to_le_bytes());
        } else {
            shape.update(text);
        }
        shape.update(b"\0");
    }
    let short =
        |h: blake3::Hasher| u64::from_le_bytes(h.finalize().as_bytes()[..8].try_into().unwrap());
    Fingerprint {
        sig: short(sig),
        body: short(body),
        shape: short(shape),
    }
}

/// A Python string statement that opens a block (module/class/function docstring).
fn is_docstring(n: Node, lang: &Language) -> bool {
    matches!(lang, Language::Python)
        && n.kind() == "expression_statement"
        && n.named_child_count() == 1
        && n.named_child(0).is_some_and(|c| c.kind() == "string")
        && n.prev_named_sibling().is_none()
        && n.parent()
            .is_some_and(|p| matches!(p.kind(), "block" | "module"))
}

fn is_callable(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Function | SymbolKind::Method)
}

fn suffix(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Class
        | SymbolKind::Interface
        | SymbolKind::Struct
        | SymbolKind::Enum
        | SymbolKind::Trait
        | SymbolKind::TypeAlias => "#",
        SymbolKind::Function | SymbolKind::Method => "().",
        SymbolKind::Variable | SymbolKind::Constant => ".",
        _ => "",
    }
}

fn id(repo: &str, descriptor: &str) -> GlobalSymbolId {
    GlobalSymbolId {
        scheme: SCHEME.to_string(),
        package_manager: ".".to_string(),
        package: repo.to_string(),
        version: ".".to_string(),
        descriptor: descriptor.to_string(),
    }
}

fn edge(src: &GlobalSymbolId, dst: &GlobalSymbolId, kind: EdgeKind, confidence: f32) -> Edge {
    Edge {
        src: src.clone(),
        dst: dst.clone(),
        kind,
        confidence,
        cross_repo: false,
    }
}

/// Make a Rust `use` path absolute: `crate::`, `self::` and `super::` against the file's module.
fn rust_absolute(module: &str, own: Option<&str>) -> String {
    let Some(own) = own else {
        return module.to_string();
    };
    let mut segs: Vec<&str> = module.split("::").collect();
    let mut base: Vec<&str> = match segs.first() {
        Some(&"crate") => vec![own.split("::").next().unwrap_or(own)],
        Some(&"self") => own.split("::").collect(),
        Some(&"super") => {
            let mut b: Vec<&str> = own.split("::").collect();
            while segs.first() == Some(&"super") {
                segs.remove(0);
                b.pop();
            }
            segs.insert(0, "");
            b
        }
        _ => return module.to_string(),
    };
    segs.remove(0);
    base.extend(segs.into_iter().filter(|s| !s.is_empty()));
    base.join("::")
}

fn join_core(core: &str, name: &str) -> String {
    if core.is_empty() {
        name.to_string()
    } else {
        format!("{core}/{name}")
    }
}

fn module_path(rel: &str) -> String {
    let no_ext = rel.rsplit_once('.').map_or(rel, |(base, _)| base);
    no_ext
        .strip_suffix("/__init__")
        .unwrap_or(no_ext)
        .to_string()
}

fn children(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

fn first_line(node: Node, src: &[u8]) -> String {
    node.utf8_text(src)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim_end()
        .to_string()
}

fn node_range(node: &Node) -> Range {
    let s = node.start_position();
    let e = node.end_position();
    Range {
        start: Position {
            line: s.row as u32 + 1,
            column: s.column as u32,
        },
        end: Position {
            line: e.row as u32 + 1,
            column: e.column as u32,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::RefKind;
    use std::io::Write;

    fn write(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        let mut f = std::fs::File::create(p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn extracts_python_class_method_and_call() {
        let dir = std::env::temp_dir().join(format!("comindly-parse-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "svc.py",
            "class Service:\n    def run(self):\n        helper()\n\ndef helper():\n    pass\n",
        );

        let out = parse_repo(&dir, "demo").unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let kinds: Vec<_> = out
            .symbols
            .iter()
            .map(|s| (s.name.as_str(), s.kind.clone()))
            .collect();
        assert!(
            kinds.contains(&("Service", SymbolKind::Class)),
            "got {kinds:?}"
        );
        assert!(
            kinds.contains(&("run", SymbolKind::Method)),
            "got {kinds:?}"
        );
        assert!(
            kinds.contains(&("helper", SymbolKind::Function)),
            "got {kinds:?}"
        );

        assert!(out.edges.iter().any(|e| e.kind == EdgeKind::Contains
            && e.src.descriptor.contains("Service#")
            && e.dst.descriptor.contains("run().")));
        let refs = &out.facts[0].refs;
        assert!(refs
            .iter()
            .any(|r| r.kind == RefKind::Call && r.from.contains("run().") && r.path == ["helper"]));
    }

    fn fp_of(body: &str, name: &str) -> Fingerprint {
        let dir = std::env::temp_dir().join(format!(
            "comindly-fp-{}-{}",
            std::process::id(),
            blake3::hash(body.as_bytes()).to_hex()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        write(&dir, "m.py", body);
        let out = parse_repo(&dir, "demo").unwrap();
        std::fs::remove_dir_all(&dir).ok();
        out.symbols
            .into_iter()
            .find(|s| s.name == name)
            .and_then(|s| s.fingerprint)
            .unwrap()
    }

    #[test]
    fn fingerprint_ignores_formatting_and_tracks_renames() {
        let base = fp_of("def f(a):\n    x = a + 1\n    return x\n", "f");
        let noisy = fp_of(
            "def f(a):\n    \"\"\"Doc.\"\"\"\n    # note\n    x = a + 1\n\n    return x\n",
            "f",
        );
        assert_eq!(base, noisy);

        let renamed = fp_of("def f(b):\n    y = b + 1\n    return y\n", "f");
        assert_ne!(base.body, renamed.body);
        assert_ne!(base.sig, renamed.sig);
        assert_eq!(base.shape, renamed.shape);

        let changed = fp_of("def f(a):\n    x = a + 2\n    return x\n", "f");
        assert_eq!(base.sig, changed.sig);
        assert_ne!(base.body, changed.body);
        assert_ne!(base.shape, changed.shape);
    }

    #[test]
    fn python_facts_capture_imports_types_and_refs() {
        let out = parse_repo(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/org/service-a"
            )),
            "service-a",
        )
        .unwrap();
        let users = out
            .facts
            .iter()
            .find(|f| f.path == "service_a/users.py")
            .unwrap();
        assert_eq!(users.modules, vec!["service_a.users"]);
        assert!(users.imports.iter().any(|i| i.module == "acme.common.http"
            && i.items
                == vec![crate::facts::ImportItem::Name {
                    name: "Client".into(),
                    alias: Some("HttpClient".into())
                }]));
        assert!(users.refs.iter().any(|r| r.path == ["self", "api", "get"]));
        assert!(users
            .refs
            .iter()
            .any(|r| r.kind == RefKind::Inherits && r.path == ["BaseService"]));
        assert!(users.types.iter().any(|t| t.name == "self.api"
            && matches!(&t.source, TypeSource::Alias { name, .. } if name == "api")));
        assert!(users
            .types
            .iter()
            .any(|t| t.name == "client"
                && t.source == TypeSource::Construct(vec!["HttpClient".into()])));
    }

    #[test]
    fn tsx_components_are_references() {
        let dir = std::env::temp_dir().join(format!("comindly-tsx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "page.tsx",
            "export function Page() {\n  return <Layout><ui.Button label=\"x\" /><div /></Layout>;\n}\n",
        );
        let out = parse_repo(&dir, "demo").unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let paths: Vec<_> = out.facts[0].refs.iter().map(|r| r.path.join(".")).collect();
        assert!(paths.contains(&"Layout".to_string()), "{paths:?}");
        assert!(paths.contains(&"ui.Button".to_string()), "{paths:?}");
        assert!(!paths.iter().any(|p| p == "div"), "{paths:?}");
    }

    #[test]
    fn typescript_facts_capture_reexports_and_heritage() {
        let kit = parse_repo(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/org/ui-kit"
            )),
            "ui-kit",
        )
        .unwrap();
        let index = kit.facts.iter().find(|f| f.path == "src/index.ts").unwrap();
        assert_eq!(index.imports.len(), 3);
        assert!(index
            .imports
            .iter()
            .any(|i| i.module == "./button" && i.items == vec![crate::facts::ImportItem::All]));

        let app = parse_repo(
            Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/org/web-app"
            )),
            "web-app",
        )
        .unwrap();
        let f = app.facts.iter().find(|f| f.path == "src/app.ts").unwrap();
        assert!(f
            .refs
            .iter()
            .any(|r| r.kind == RefKind::Implements && r.path == ["Theme"]));
        assert!(f.refs.iter().any(|r| r.path == ["kit", "formatDate"]));
        assert!(f.imports.iter().any(|i| i.items
            == vec![crate::facts::ImportItem::Module {
                alias: Some("kit".into())
            }]));
    }
}
