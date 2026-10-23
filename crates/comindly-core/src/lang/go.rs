//! Go syntax adapter (tree-sitter-go). A package is a directory; methods attach to receivers.

use tree_sitter::Node;

use super::{field_text, line, named_children, text, LangSpec, RawContract, Target};
use crate::facts::{Args, Import, ImportItem, RefKind, TypeSource, ValueExpr};
use crate::model::SymbolKind;

pub struct Go;

/// Router registration methods across net/http, gin, echo and chi.
const ROUTE_METHODS: &[&str] = &[
    "HandleFunc",
    "Handle",
    "GET",
    "POST",
    "PUT",
    "PATCH",
    "DELETE",
    "Get",
    "Post",
    "Put",
    "Patch",
    "Delete",
];

impl LangSpec for Go {
    fn receiver(&self) -> &'static str {
        "self"
    }

    fn contract(&self, node: Node, src: &[u8]) -> Option<RawContract> {
        // `api := r.Group("/api")` (gin, echo): a route group with a prefix.
        if node.kind() == "short_var_declaration" {
            let left = named_children(node.child_by_field_name("left")?);
            let right = named_children(node.child_by_field_name("right")?);
            let ([name], [call]) = (left.as_slice(), right.as_slice()) else {
                return None;
            };
            if name.kind() != "identifier" || call.kind() != "call_expression" {
                return None;
            }
            let ctor = path(call.child_by_field_name("function")?, src)?;
            if ctor.last().map(String::as_str) != Some("Group") {
                return None;
            }
            return Some(RawContract::Construct {
                target: Target::Var(text(*name, src).to_string()),
                ctor,
                args: args(call.child_by_field_name("arguments")?, src),
            });
        }
        if node.kind() != "call_expression" {
            return None;
        }
        let f = node.child_by_field_name("function")?;
        let p = path(f, src)?;
        let (method, receiver) = p.split_last()?;
        let list = node.child_by_field_name("arguments")?;
        let args = args(list, src);
        if receiver == ["http"] && !method.starts_with("Handle") {
            return Some(RawContract::Op {
                receiver: receiver.to_vec(),
                method: method.clone(),
                args,
            });
        }
        if !ROUTE_METHODS.contains(&method.as_str()) || args.positional.len() < 2 {
            return None;
        }
        let Some(ValueExpr::Lit(pattern)) = args.positional.first().cloned().flatten() else {
            return None;
        };
        let (verb, route) = match pattern.split_once(' ') {
            Some((v, r)) if v.chars().all(|c| c.is_ascii_uppercase()) => (v.to_string(), r),
            _ if method.starts_with("Handle") => ("ANY".to_string(), pattern.as_str()),
            _ => (method.to_uppercase(), pattern.as_str()),
        };
        if !route.starts_with('/') {
            return None;
        }
        let handler = named_children(list)
            .last()
            .filter(|h| h.kind() != "func_literal")
            .and_then(|h| path(*h, src))
            .unwrap_or_default();
        let (prefix, receiver) = route_scope(node, receiver.to_vec(), src);
        Some(RawContract::Handle {
            receiver,
            method: verb,
            path: ValueExpr::Lit(format!("{prefix}{route}")),
            handler,
        })
    }

    fn value(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        if !matches!(node.kind(), "const_spec" | "var_spec") {
            return None;
        }
        let name = field_text(node, "name", src)?.to_string();
        let v = named_children(node.child_by_field_name("value")?)
            .into_iter()
            .next()?;
        Some((name, value(v, src)?))
    }

    fn def(&self, node: Node, src: &[u8], _in_class: bool) -> Option<(SymbolKind, String)> {
        let kind = match node.kind() {
            "function_declaration" => SymbolKind::Function,
            "method_declaration" | "method_elem" => SymbolKind::Method,
            "type_spec" => match node.child_by_field_name("type")?.kind() {
                "struct_type" => SymbolKind::Struct,
                "interface_type" => SymbolKind::Interface,
                _ => SymbolKind::TypeAlias,
            },
            "type_alias" => SymbolKind::TypeAlias,
            _ => return None,
        };
        Some((kind, field_text(node, "name", src)?.to_string()))
    }

    fn owner(&self, def: Node, src: &[u8]) -> Option<String> {
        if def.kind() != "method_declaration" {
            return None;
        }
        let recv = named_children(def.child_by_field_name("receiver")?)
            .into_iter()
            .find(|p| p.kind() == "parameter_declaration")?;
        type_path(recv.child_by_field_name("type")?, src)?.pop()
    }

    fn variable(&self, node: Node, src: &[u8]) -> Option<String> {
        matches!(node.kind(), "var_spec" | "const_spec")
            .then(|| field_text(node, "name", src).map(str::to_string))?
    }

    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>> {
        if node.kind() != "import_declaration" {
            return None;
        }
        let mut specs = Vec::new();
        for c in named_children(node) {
            match c.kind() {
                "import_spec" => specs.push(c),
                "import_spec_list" => specs.extend(
                    named_children(c)
                        .into_iter()
                        .filter(|s| s.kind() == "import_spec"),
                ),
                _ => {}
            }
        }
        let imports = specs
            .into_iter()
            .filter_map(|s| {
                let module = field_text(s, "path", src)?
                    .trim_matches(|c| c == '"' || c == '`')
                    .to_string();
                let item = match s
                    .child_by_field_name("name")
                    .map(|n| (n.kind(), text(n, src)))
                {
                    Some(("dot", _)) => ImportItem::All,
                    Some(("blank_identifier", _)) => ImportItem::Module { alias: None },
                    Some((_, alias)) => ImportItem::Module {
                        alias: Some(alias.to_string()),
                    },
                    None => ImportItem::Module {
                        alias: Some(default_name(&module).to_string()),
                    },
                };
                Some(Import {
                    module,
                    level: 0,
                    items: vec![item],
                    line: line(s),
                    file: None,
                })
            })
            .collect();
        Some(imports)
    }

    fn reference(&self, node: Node, src: &[u8]) -> Option<(RefKind, Vec<String>)> {
        match node.kind() {
            "call_expression" => Some((
                RefKind::Call,
                path(node.child_by_field_name("function")?, src)?,
            )),
            "composite_literal" => Some((
                RefKind::New,
                type_path(node.child_by_field_name("type")?, src)?,
            )),
            _ => None,
        }
    }

    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)> {
        match node.kind() {
            "short_var_declaration" => {
                let left = named_children(node.child_by_field_name("left")?)
                    .into_iter()
                    .next()?;
                if left.kind() != "identifier" {
                    return None;
                }
                let right = named_children(node.child_by_field_name("right")?)
                    .into_iter()
                    .next()?;
                Some((
                    Target::Var(text(left, src).to_string()),
                    value_type(right, src)?,
                ))
            }
            "var_spec" => {
                let name = field_text(node, "name", src)?.to_string();
                if let Some(t) = node
                    .child_by_field_name("type")
                    .and_then(|t| type_path(t, src))
                {
                    return Some((Target::Var(name), TypeSource::Annotation(t)));
                }
                let v = named_children(node.child_by_field_name("value")?)
                    .into_iter()
                    .next()?;
                Some((Target::Var(name), value_type(v, src)?))
            }
            "field_declaration" => Some((
                Target::Attr(field_text(node, "name", src)?.to_string()),
                TypeSource::Annotation(type_path(node.child_by_field_name("type")?, src)?),
            )),
            _ => None,
        }
    }

    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)> {
        let Some(ty) = (def.kind() == "type_spec")
            .then(|| def.child_by_field_name("type"))
            .flatten()
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut stack = vec![ty];
        while let Some(n) = stack.pop() {
            match n.kind() {
                // An embedded field (type only, no name) promotes the embedded type's methods.
                "field_declaration" if n.child_by_field_name("name").is_none() => {
                    if let Some(p) = n
                        .child_by_field_name("type")
                        .and_then(|t| type_path(t, src))
                    {
                        out.push((RefKind::Inherits, p));
                    }
                }
                "type_elem" => out.extend(
                    named_children(n)
                        .into_iter()
                        .filter_map(|t| type_path(t, src))
                        .map(|p| (RefKind::Inherits, p)),
                ),
                "struct_type" | "interface_type" | "field_declaration_list" => {
                    stack.extend(named_children(n))
                }
                _ => {}
            }
        }
        out
    }

    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>> {
        let r = def.child_by_field_name("result")?;
        if r.kind() == "parameter_list" {
            let first = named_children(r)
                .into_iter()
                .find(|p| p.kind() == "parameter_declaration")?;
            return type_path(first.child_by_field_name("type")?, src);
        }
        type_path(r, src)
    }

    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)> {
        let lists = ["receiver", "parameters"]
            .into_iter()
            .filter_map(|f| def.child_by_field_name(f));
        let mut out = Vec::new();
        for list in lists {
            for p in named_children(list)
                .into_iter()
                .filter(|p| p.kind() == "parameter_declaration")
            {
                let Some(ty) = p
                    .child_by_field_name("type")
                    .and_then(|t| type_path(t, src))
                else {
                    continue;
                };
                let mut cur = p.walk();
                for n in p.children_by_field_name("name", &mut cur) {
                    out.push((text(n, src).to_string(), ty.clone(), false));
                }
            }
        }
        out
    }
}

/// Package name an import binds by default: the last path segment, skipping `/vN`.
fn default_name(module: &str) -> &str {
    let mut segs = module.rsplit('/');
    let last = segs.next().unwrap_or(module);
    let is_version =
        last.len() > 1 && last.starts_with('v') && last[1..].chars().all(|c| c.is_ascii_digit());
    let name = if is_version {
        segs.next().unwrap_or(last)
    } else {
        last
    };
    // gopkg.in/yaml.v3 → yaml
    name.split('.').next().unwrap_or(name)
}

/// Type of an initializer: a call (its return type), a composite literal, or `&T{}`.
fn value_type(v: Node, src: &[u8]) -> Option<TypeSource> {
    match v.kind() {
        "call_expression" => Some(TypeSource::Construct(path(
            v.child_by_field_name("function")?,
            src,
        )?)),
        "composite_literal" => Some(TypeSource::Construct(type_path(
            v.child_by_field_name("type")?,
            src,
        )?)),
        "unary_expression" => value_type(v.child_by_field_name("operand")?, src),
        "identifier" => Some(TypeSource::Alias {
            name: text(v, src).to_string(),
            scope: String::new(),
        }),
        _ => None,
    }
}

/// chi's `r.Route("/api", func(r chi.Router) { r.Get(...) })`: the prefixes of the enclosing
/// `Route` closures whose parameter is this receiver, and the outermost receiver.
fn route_scope(node: Node, mut receiver: Vec<String>, src: &[u8]) -> (String, Vec<String>) {
    let mut prefix = String::new();
    let mut cur = node.parent();
    while let Some(n) = cur {
        cur = n.parent();
        if n.kind() != "func_literal" {
            continue;
        }
        let param = n
            .child_by_field_name("parameters")
            .and_then(|ps| named_children(ps).into_iter().next())
            .and_then(|p| p.child_by_field_name("name"))
            .map(|i| text(i, src).to_string());
        let call = n
            .parent()
            .filter(|a| a.kind() == "argument_list")
            .and_then(|a| a.parent())
            .filter(|c| c.kind() == "call_expression");
        let (Some(param), Some(call)) = (param, call) else {
            continue;
        };
        if receiver != [param] {
            continue;
        }
        let Some(p) = call
            .child_by_field_name("function")
            .and_then(|f| path(f, src))
        else {
            continue;
        };
        let Some((m, outer)) = p.split_last() else {
            continue;
        };
        let lit = call
            .child_by_field_name("arguments")
            .and_then(|a| named_children(a).into_iter().next())
            .and_then(|a| value(a, src));
        if let (true, Some(ValueExpr::Lit(l))) = (m == "Route", lit) {
            prefix = format!("{l}{prefix}");
            receiver = outer.to_vec();
        }
    }
    (prefix, receiver)
}

fn value(node: Node, src: &[u8]) -> Option<ValueExpr> {
    match node.kind() {
        "interpreted_string_literal" | "raw_string_literal" => Some(ValueExpr::Lit(
            text(node, src)
                .trim_matches(|c| c == '"' || c == '`')
                .to_string(),
        )),
        "identifier" | "selector_expression" => path(node, src).map(ValueExpr::Name),
        "binary_expression" if field_text(node, "operator", src) == Some("+") => {
            Some(ValueExpr::Join {
                sep: String::new(),
                parts: [
                    node.child_by_field_name("left"),
                    node.child_by_field_name("right"),
                ]
                .into_iter()
                .map(|n| {
                    n.and_then(|n| value(n, src))
                        .unwrap_or(ValueExpr::Name(Vec::new()))
                })
                .collect(),
            })
        }
        "call_expression" => {
            let func = path(node.child_by_field_name("function")?, src)?;
            let a = args(node.child_by_field_name("arguments")?, src);
            Some(ValueExpr::Call {
                func,
                args: a
                    .positional
                    .into_iter()
                    .map(|v| v.unwrap_or(ValueExpr::Name(Vec::new())))
                    .collect(),
                kwargs: Vec::new(),
            })
        }
        "parenthesized_expression" => value(*named_children(node).first()?, src),
        _ => None,
    }
}

fn args(list: Node, src: &[u8]) -> Args {
    let mut out = Args::default();
    for a in named_children(list) {
        if a.kind() == "comment" {
            continue;
        }
        out.positional.push(value(a, src));
    }
    out
}

fn path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" | "field_identifier" | "type_identifier" | "package_identifier" => {
            Some(vec![text(node, src).to_string()])
        }
        "selector_expression" => {
            let mut p = path(node.child_by_field_name("operand")?, src)?;
            p.push(field_text(node, "field", src)?.to_string());
            Some(p)
        }
        _ => None,
    }
}

fn type_path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "qualified_type" => Some(vec![
            field_text(node, "package", src)?.to_string(),
            field_text(node, "name", src)?.to_string(),
        ]),
        "pointer_type" | "parenthesized_type" => {
            type_path(named_children(node).into_iter().next()?, src)
        }
        "generic_type" => type_path(node.child_by_field_name("type")?, src),
        _ => path(node, src),
    }
}

#[cfg(test)]
mod tests {
    use super::default_name;

    #[test]
    fn go_default_import_names() {
        assert_eq!(default_name("github.com/acme/golib/store"), "store");
        assert_eq!(default_name("github.com/jackc/pgx/v5"), "pgx");
        assert_eq!(default_name("gopkg.in/yaml.v3"), "yaml");
        assert_eq!(default_name("fmt"), "fmt");
    }
}
