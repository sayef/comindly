//! Rust syntax adapter (tree-sitter-rust). Modules follow files; methods attach via `impl`.

use tree_sitter::Node;

use super::{field_text, line, named_children, text, LangSpec, RawContract, Target};
use crate::facts::{Args, Import, ImportItem, RefKind, TemplatePart, TypeSource, ValueExpr};
use crate::model::SymbolKind;

pub struct Rust;

/// Generic wrappers whose first type argument is the value's real type.
const WRAPPERS: &[&str] = &[
    "Box", "Arc", "Rc", "Option", "Result", "RefCell", "Mutex", "RwLock", "Cow",
];

const VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "head"];

impl LangSpec for Rust {
    fn receiver(&self) -> &'static str {
        "self"
    }

    fn contracts(&self, node: Node, src: &[u8]) -> Vec<RawContract> {
        match node.kind() {
            // actix / rocket: `#[get("/x")] async fn h()`.
            "function_item" => attribute_route(node, src).into_iter().collect(),
            "call_expression" => {
                let Some(f) = node.child_by_field_name("function") else {
                    return Vec::new();
                };
                let Some(list) = node.child_by_field_name("arguments") else {
                    return Vec::new();
                };
                let args = named_children(list);
                // axum: `.route("/x", get(h).post(h2))`.
                if f.kind() == "field_expression" && field_text(f, "field", src) == Some("route") {
                    if let [p, chain] = args.as_slice() {
                        if let Some(ValueExpr::Lit(path)) = value(*p, src) {
                            let mut out = Vec::new();
                            method_router(*chain, src, &mut out);
                            return out
                                .into_iter()
                                .map(|(verb, handler)| RawContract::Handle {
                                    receiver: Vec::new(),
                                    method: verb.to_uppercase(),
                                    path: ValueExpr::Lit(path.clone()),
                                    handler,
                                })
                                .collect();
                        }
                    }
                    return Vec::new();
                }
                // reqwest: `client.get(url)`, `reqwest::get(url)`.
                let Some(mut p) = path(f, src) else {
                    return Vec::new();
                };
                let Some(method) = p.pop() else {
                    return Vec::new();
                };
                if !VERBS.contains(&method.as_str()) || args.is_empty() {
                    return Vec::new();
                }
                vec![RawContract::Op {
                    receiver: p,
                    method,
                    args: Args {
                        positional: args.iter().map(|n| value(*n, src)).collect(),
                        ..Default::default()
                    },
                }]
            }
            _ => Vec::new(),
        }
    }

    fn value(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        if !matches!(node.kind(), "const_item" | "static_item") {
            return None;
        }
        Some((
            field_text(node, "name", src)?.to_string(),
            value(node.child_by_field_name("value")?, src)?,
        ))
    }

    fn def(&self, node: Node, src: &[u8], _in_class: bool) -> Option<(SymbolKind, String)> {
        let kind = match node.kind() {
            "function_item" if in_impl_or_trait(node) => SymbolKind::Method,
            "function_item" => SymbolKind::Function,
            "function_signature_item" => SymbolKind::Method,
            "struct_item" | "union_item" => SymbolKind::Struct,
            "enum_item" => SymbolKind::Enum,
            "enum_variant" => SymbolKind::Constant,
            "trait_item" => SymbolKind::Trait,
            "type_item" => SymbolKind::TypeAlias,
            // Inline `mod name { … }`; `mod name;` only declares a file module.
            "mod_item" if node.child_by_field_name("body").is_some() => SymbolKind::Module,
            _ => return None,
        };
        Some((kind, field_text(node, "name", src)?.to_string()))
    }

    fn owner(&self, def: Node, src: &[u8]) -> Option<String> {
        if !matches!(def.kind(), "function_item" | "function_signature_item") {
            return None;
        }
        let imp = def.parent()?.parent()?;
        if imp.kind() != "impl_item" {
            return None;
        }
        type_path(imp.child_by_field_name("type")?, src)?.pop()
    }

    fn owned_bases(&self, node: Node, src: &[u8]) -> Option<(String, RefKind, Vec<String>)> {
        if node.kind() != "impl_item" {
            return None;
        }
        let ty = type_path(node.child_by_field_name("type")?, src)?.pop()?;
        let tr = type_path(node.child_by_field_name("trait")?, src)?;
        Some((ty, RefKind::Implements, tr))
    }

    fn variable(&self, node: Node, src: &[u8]) -> Option<String> {
        matches!(node.kind(), "const_item" | "static_item")
            .then(|| field_text(node, "name", src).map(str::to_string))?
    }

    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>> {
        if node.kind() != "use_declaration" {
            return None;
        }
        let mut flat = Vec::new();
        flatten_use(node.child_by_field_name("argument")?, &[], src, &mut flat);
        Some(
            flat.into_iter()
                .map(|(module, item)| Import {
                    module: module.join("::"),
                    level: 0,
                    items: vec![item],
                    line: line(node),
                    file: None,
                })
                .collect(),
        )
    }

    fn reference(&self, node: Node, src: &[u8]) -> Option<(RefKind, Vec<String>)> {
        match node.kind() {
            "call_expression" => Some((
                RefKind::Call,
                path(node.child_by_field_name("function")?, src)?,
            )),
            "struct_expression" => Some((
                RefKind::New,
                type_path(node.child_by_field_name("name")?, src)?,
            )),
            _ => None,
        }
    }

    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)> {
        match node.kind() {
            "let_declaration" => {
                let pat = node.child_by_field_name("pattern")?;
                if pat.kind() != "identifier" {
                    return None;
                }
                let target = Target::Var(text(pat, src).to_string());
                if let Some(t) = node
                    .child_by_field_name("type")
                    .and_then(|t| type_path(t, src))
                {
                    return Some((target, TypeSource::Annotation(t)));
                }
                Some((target, value_type(node.child_by_field_name("value")?, src)?))
            }
            "field_declaration" => Some((
                Target::Attr(field_text(node, "name", src)?.to_string()),
                TypeSource::Annotation(type_path(node.child_by_field_name("type")?, src)?),
            )),
            _ => None,
        }
    }

    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)> {
        // Supertraits: `trait A: B + C`.
        def.child_by_field_name("bounds")
            .filter(|_| def.kind() == "trait_item")
            .map(|b| {
                named_children(b)
                    .into_iter()
                    .filter_map(|t| type_path(t, src))
                    .map(|p| (RefKind::Inherits, p))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>> {
        type_path(def.child_by_field_name("return_type")?, src)
    }

    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)> {
        let Some(ps) = def.child_by_field_name("parameters") else {
            return Vec::new();
        };
        named_children(ps)
            .into_iter()
            .filter(|p| p.kind() == "parameter")
            .filter_map(|p| {
                let pat = p.child_by_field_name("pattern")?;
                (pat.kind() == "identifier").then_some(())?;
                Some((
                    text(pat, src).to_string(),
                    type_path(p.child_by_field_name("type")?, src)?,
                    false,
                ))
            })
            .collect()
    }
}

fn in_impl_or_trait(node: Node) -> bool {
    node.parent()
        .and_then(|p| p.parent())
        .is_some_and(|g| matches!(g.kind(), "impl_item" | "trait_item"))
}

/// Flatten a `use` tree into (module path, item) pairs.
fn flatten_use(
    node: Node,
    prefix: &[String],
    src: &[u8],
    out: &mut Vec<(Vec<String>, ImportItem)>,
) {
    let with = |segs: Vec<String>| -> Vec<String> {
        let mut p = prefix.to_vec();
        p.extend(segs);
        p
    };
    match node.kind() {
        "identifier" | "crate" | "super" => {
            let name = text(node, src).to_string();
            if prefix.is_empty() {
                out.push((vec![name.clone()], ImportItem::Module { alias: Some(name) }));
            } else {
                out.push((prefix.to_vec(), ImportItem::Name { name, alias: None }));
            }
        }
        // `use a::b::{self, …}` binds the module `b`.
        "self" => out.push((
            prefix.to_vec(),
            ImportItem::Module {
                alias: prefix.last().cloned(),
            },
        )),
        "scoped_identifier" => {
            if let Some(mut segs) = segments(node, src) {
                let name = segs.pop().unwrap_or_default();
                if name == "self" {
                    let alias = segs.last().cloned();
                    out.push((with(segs), ImportItem::Module { alias }));
                } else {
                    out.push((with(segs), ImportItem::Name { name, alias: None }));
                }
            }
        }
        "use_as_clause" => {
            let alias = field_text(node, "alias", src).map(str::to_string);
            let Some(mut segs) = node
                .child_by_field_name("path")
                .and_then(|p| segments(p, src))
            else {
                return;
            };
            if segs.len() == 1 && prefix.is_empty() {
                out.push((segs, ImportItem::Module { alias }));
            } else {
                let name = segs.pop().unwrap_or_default();
                out.push((with(segs), ImportItem::Name { name, alias }));
            }
        }
        "use_wildcard" => {
            if let Some(segs) = named_children(node)
                .into_iter()
                .next()
                .and_then(|p| segments(p, src))
            {
                out.push((with(segs), ImportItem::All));
            }
        }
        "scoped_use_list" => {
            let base = node
                .child_by_field_name("path")
                .and_then(|p| segments(p, src))
                .unwrap_or_default();
            if let Some(list) = node.child_by_field_name("list") {
                let p = with(base);
                for c in named_children(list) {
                    flatten_use(c, &p, src, out);
                }
            }
        }
        "use_list" => {
            for c in named_children(node) {
                flatten_use(c, prefix, src, out);
            }
        }
        _ => {}
    }
}

/// `a::b::c` (incl. `crate`, `self`, `super`) as segments.
fn segments(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" | "crate" | "self" | "super" | "type_identifier" => {
            Some(vec![text(node, src).to_string()])
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut p = match node.child_by_field_name("path") {
                Some(p) => segments(p, src)?,
                None => Vec::new(),
            };
            p.push(field_text(node, "name", src)?.to_string());
            Some(p)
        }
        _ => None,
    }
}

/// Expression path; a leading module `self::` becomes `<self>` (vs. the `self` receiver).
fn attribute_route(func: Node, src: &[u8]) -> Option<RawContract> {
    let mut prev = func.prev_named_sibling();
    while let Some(a) = prev.filter(|p| matches!(p.kind(), "attribute_item" | "line_comment")) {
        prev = a.prev_named_sibling();
        let Some(attr) = named_children(a)
            .into_iter()
            .find(|c| c.kind() == "attribute")
        else {
            continue;
        };
        let kids = named_children(attr);
        let verb = kids.first().and_then(|n| path(*n, src))?.pop()?;
        if !VERBS.contains(&verb.as_str()) {
            continue;
        }
        let lit = kids
            .iter()
            .find(|k| k.kind() == "token_tree")
            .and_then(|t| {
                named_children(*t)
                    .into_iter()
                    .find(|c| c.kind() == "string_literal")
            })
            .and_then(|s| value(s, src))?;
        return Some(RawContract::Route {
            handler: field_text(func, "name", src)?.to_string(),
            receiver: Vec::new(),
            method: verb.to_uppercase(),
            path: lit,
            client: false,
        });
    }
    None
}

/// `get(h).post(h2)` → [(get, h), (post, h2)].
fn method_router(node: Node, src: &[u8], out: &mut Vec<(String, Vec<String>)>) {
    if node.kind() != "call_expression" {
        return;
    }
    let (Some(f), Some(list)) = (
        node.child_by_field_name("function"),
        node.child_by_field_name("arguments"),
    ) else {
        return;
    };
    let (verb, inner) = match f.kind() {
        "field_expression" => (
            field_text(f, "field", src).unwrap_or_default().to_string(),
            f.child_by_field_name("value"),
        ),
        _ => (
            path(f, src).and_then(|mut p| p.pop()).unwrap_or_default(),
            None,
        ),
    };
    if let Some(i) = inner {
        method_router(i, src, out);
    }
    if VERBS.contains(&verb.as_str()) {
        let handler = named_children(list)
            .first()
            .filter(|h| h.kind() != "closure_expression")
            .and_then(|h| path(*h, src))
            .unwrap_or_default();
        out.push((verb, handler));
    }
}

/// Literals, names, and `format!` templates with inline or positional arguments.
fn value(node: Node, src: &[u8]) -> Option<ValueExpr> {
    match node.kind() {
        "string_literal" | "raw_string_literal" => Some(ValueExpr::Lit(
            named_children(node)
                .iter()
                .filter(|c| c.kind() == "string_content")
                .map(|c| text(*c, src))
                .collect(),
        )),
        "identifier" | "scoped_identifier" => path(node, src).map(ValueExpr::Name),
        "reference_expression" => value(node.child_by_field_name("value")?, src),
        "macro_invocation" if field_text(node, "macro", src) == Some("format") => {
            let tt = named_children(node)
                .into_iter()
                .find(|c| c.kind() == "token_tree")?;
            let kids = named_children(tt);
            let fmt = value(*kids.first().filter(|k| k.kind() == "string_literal")?, src)?;
            let ValueExpr::Lit(fmt) = fmt else {
                return None;
            };
            // One slot per argument, so a non-identifier keeps its position.
            let mut positional = kids[1..]
                .iter()
                .map(|k| (k.kind() == "identifier").then(|| text(*k, src).to_string()));
            let mut parts = Vec::new();
            let mut rest = fmt.as_str();
            while let Some(open) = rest.find('{') {
                parts.push(TemplatePart::Lit(rest[..open].to_string()));
                let close = rest[open..].find('}').map_or(rest.len(), |c| open + c);
                let name = rest[open + 1..close].split(':').next().unwrap_or("");
                let expr = if name.is_empty() {
                    positional.next().flatten().map(|n| vec![n])
                } else {
                    Some(vec![name.to_string()])
                };
                parts.push(TemplatePart::Expr(expr));
                rest = rest.get(close + 1..).unwrap_or("");
            }
            parts.push(TemplatePart::Lit(rest.to_string()));
            Some(ValueExpr::Template(parts))
        }
        _ => None,
    }
}

fn path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" | "self" | "crate" | "super" => Some(vec![text(node, src).to_string()]),
        "scoped_identifier" => {
            let mut p = segments(node, src)?;
            if p.first().is_some_and(|s| s == "self") {
                p[0] = "<self>".into();
            }
            Some(p)
        }
        "field_expression" => {
            let mut p = path(node.child_by_field_name("value")?, src)?;
            p.push(field_text(node, "field", src)?.to_string());
            Some(p)
        }
        "generic_function" => path(node.child_by_field_name("function")?, src),
        _ => None,
    }
}

fn type_path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "type_identifier" | "identifier" => Some(vec![text(node, src).to_string()]),
        "scoped_type_identifier" | "scoped_identifier" => segments(node, src),
        "reference_type" => type_path(node.child_by_field_name("type")?, src),
        "generic_type" | "generic_type_with_turbofish" => {
            let base = type_path(node.child_by_field_name("type")?, src)?;
            if base.last().is_some_and(|b| WRAPPERS.contains(&b.as_str())) {
                let args = node.child_by_field_name("type_arguments")?;
                return named_children(args)
                    .into_iter()
                    .find_map(|a| type_path(a, src));
            }
            Some(base)
        }
        _ => None,
    }
}

/// Type of a `let` initializer: a call (its return type) or a struct literal.
fn value_type(v: Node, src: &[u8]) -> Option<TypeSource> {
    match v.kind() {
        "call_expression" => Some(TypeSource::Construct(path(
            v.child_by_field_name("function")?,
            src,
        )?)),
        "struct_expression" => Some(TypeSource::Construct(type_path(
            v.child_by_field_name("name")?,
            src,
        )?)),
        "try_expression" | "await_expression" | "reference_expression" => {
            value_type(named_children(v).into_iter().next()?, src)
        }
        "identifier" => Some(TypeSource::Alias {
            name: text(v, src).to_string(),
            scope: String::new(),
        }),
        _ => None,
    }
}
