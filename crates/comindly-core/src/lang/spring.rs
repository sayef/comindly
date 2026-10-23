//! Spring MVC and OpenFeign contracts for Java and Kotlin: mapping annotations on controller
//! methods serve routes, the same annotations on `@FeignClient` interfaces consume them.

use tree_sitter::Node;

use super::{named_children, text, RawContract};
use crate::facts::{Args, ValueExpr};

const MAPPINGS: &[(&str, &str)] = &[
    ("GetMapping", "GET"),
    ("PostMapping", "POST"),
    ("PutMapping", "PUT"),
    ("PatchMapping", "PATCH"),
    ("DeleteMapping", "DELETE"),
];

/// RestTemplate methods and the verb they imply (`exchange` takes it as an argument).
pub(crate) const REST_TEMPLATE: &[(&str, &str)] = &[
    ("getForObject", "GET"),
    ("getForEntity", "GET"),
    ("postForObject", "POST"),
    ("postForEntity", "POST"),
    ("postForLocation", "POST"),
    ("patchForObject", "PATCH"),
    ("exchange", ""),
];

/// Receiver marking a WebClient request (`client.get().uri(url)`); the method is the verb.
pub(crate) const WEBCLIENT: &str = "<webclient>";
const VERBS: &[&str] = &["get", "post", "put", "patch", "delete"];

/// `x.get().uri(...)`: the verb of the call a `uri(...)` call is chained on.
pub(crate) fn webclient_verb(inner_method: &str) -> Option<String> {
    VERBS
        .contains(&inner_method)
        .then(|| inner_method.to_uppercase())
}

struct Annotation<'t> {
    name: String,
    args: Vec<(Option<String>, Node<'t>)>,
}

/// A route declared by a mapping annotation on a Java method or Kotlin function.
pub(crate) fn route(def: Node, src: &[u8]) -> Option<RawContract> {
    if !matches!(def.kind(), "method_declaration" | "function_declaration") {
        return None;
    }
    let anns = annotations(def, src);
    let (verb, path) = anns.iter().find_map(|a| {
        if let Some((_, v)) = MAPPINGS.iter().find(|(n, _)| *n == a.name) {
            return Some((v.to_string(), mapping_path(a, src)));
        }
        if a.name == "RequestMapping" {
            let verb = arg(a, "method", usize::MAX)
                .and_then(|n| value(n, src))
                .and_then(|v| match v {
                    ValueExpr::Name(p) => p.last().cloned(),
                    _ => None,
                })
                .unwrap_or_else(|| "ANY".into());
            return Some((verb, mapping_path(a, src)));
        }
        None
    })?;
    let class = def
        .parent()
        .filter(|b| matches!(b.kind(), "class_body" | "interface_body"))
        .and_then(|b| b.parent())?;
    let class_anns = annotations(class, src);
    let client = class_anns.iter().any(|a| a.name == "FeignClient");
    let prefix = class_anns.iter().find_map(|a| match a.name.as_str() {
        "RequestMapping" => Some(mapping_path(a, src)),
        "FeignClient" => arg(a, "path", usize::MAX).and_then(|n| value(n, src)),
        _ => None,
    });
    let handler = text(
        def.child_by_field_name("name").or_else(|| name_of(def))?,
        src,
    );
    Some(RawContract::Route {
        handler: handler.to_string(),
        receiver: Vec::new(),
        method: verb.to_uppercase(),
        path: ValueExpr::Join {
            sep: String::new(),
            parts: vec![prefix.unwrap_or(ValueExpr::Lit(String::new())), path],
        },
        client,
    })
}

fn name_of(def: Node) -> Option<Node> {
    named_children(def)
        .into_iter()
        .find(|c| matches!(c.kind(), "identifier" | "simple_identifier"))
}

fn mapping_path(a: &Annotation<'_>, src: &[u8]) -> ValueExpr {
    arg(a, "value", 0)
        .or_else(|| arg(a, "path", usize::MAX))
        .and_then(|n| value(n, src))
        .unwrap_or(ValueExpr::Lit(String::new()))
}

fn arg<'t>(a: &Annotation<'t>, key: &str, pos: usize) -> Option<Node<'t>> {
    a.args
        .iter()
        .find(|(k, _)| k.as_deref() == Some(key))
        .or_else(|| a.args.iter().filter(|(k, _)| k.is_none()).nth(pos))
        .map(|(_, n)| *n)
}

fn annotations<'t>(def: Node<'t>, src: &[u8]) -> Vec<Annotation<'t>> {
    let mut out = Vec::new();
    for m in named_children(def)
        .into_iter()
        .filter(|c| c.kind() == "modifiers")
    {
        for a in named_children(m) {
            if let Some(ann) = annotation(a, src) {
                out.push(ann);
            }
        }
    }
    out
}

fn annotation<'t>(a: Node<'t>, src: &[u8]) -> Option<Annotation<'t>> {
    match a.kind() {
        // Java
        "marker_annotation" | "annotation" if a.child_by_field_name("name").is_some() => {
            let name = last(text(a.child_by_field_name("name")?, src));
            let mut args = Vec::new();
            if let Some(list) = a.child_by_field_name("arguments") {
                for c in named_children(list) {
                    if c.kind() == "element_value_pair" {
                        let k = c
                            .child_by_field_name("key")
                            .map(|k| text(k, src).to_string());
                        if let Some(v) = c.child_by_field_name("value") {
                            args.push((k, v));
                        }
                    } else {
                        args.push((None, c));
                    }
                }
            }
            Some(Annotation { name, args })
        }
        // Kotlin
        "annotation" => {
            let inner = named_children(a).into_iter().next()?;
            match inner.kind() {
                "user_type" => Some(Annotation {
                    name: last(text(inner, src)),
                    args: Vec::new(),
                }),
                "constructor_invocation" => {
                    let kids = named_children(inner);
                    let name = last(text(*kids.first()?, src));
                    let mut args = Vec::new();
                    if let Some(list) = kids.iter().find(|k| k.kind() == "value_arguments") {
                        for va in named_children(*list) {
                            let parts = named_children(va);
                            match parts.as_slice() {
                                [k, v] if k.kind() == "identifier" => {
                                    args.push((Some(text(*k, src).to_string()), *v))
                                }
                                [v] => args.push((None, *v)),
                                _ => {}
                            }
                        }
                    }
                    Some(Annotation { name, args })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn last(s: &str) -> String {
    s.rsplit('.').next().unwrap_or(s).trim().to_string()
}

/// A statically readable Java/Kotlin expression: literals, names, `+` concatenation,
/// `String.format`, and the first element of an array/collection literal.
pub(crate) fn value(node: Node, src: &[u8]) -> Option<ValueExpr> {
    match node.kind() {
        "string_literal" => {
            let kids = named_children(node);
            if kids
                .iter()
                .all(|k| matches!(k.kind(), "string_fragment" | "string_content"))
            {
                return Some(ValueExpr::Lit(
                    kids.iter().map(|k| text(*k, src)).collect::<String>(),
                ));
            }
            Some(ValueExpr::Join {
                sep: String::new(),
                parts: kids
                    .iter()
                    .map(|k| match k.kind() {
                        "string_fragment" | "string_content" => {
                            ValueExpr::Lit(text(*k, src).to_string())
                        }
                        _ => named_children(*k)
                            .first()
                            .and_then(|e| value(*e, src))
                            .unwrap_or(ValueExpr::Name(Vec::new())),
                    })
                    .collect(),
            })
        }
        "identifier" | "simple_identifier" => Some(ValueExpr::Name(vec![text(node, src).into()])),
        "field_access" | "navigation_expression" => Some(ValueExpr::Name(
            text(node, src)
                .split('.')
                .map(|s| s.trim().to_string())
                .collect(),
        )),
        "binary_expression" | "additive_expression" => {
            let kids = named_children(node);
            if !text(node, src).contains('+') || kids.len() != 2 {
                return None;
            }
            Some(ValueExpr::Join {
                sep: String::new(),
                parts: kids
                    .iter()
                    .map(|k| value(*k, src).unwrap_or(ValueExpr::Name(Vec::new())))
                    .collect(),
            })
        }
        "element_value_array_initializer" | "collection_literal" | "array_initializer" => {
            value(*named_children(node).first()?, src)
        }
        "parenthesized_expression" => value(*named_children(node).first()?, src),
        _ => None,
    }
}

/// Call arguments as values (Java `argument_list`, Kotlin `value_arguments`).
pub(crate) fn args(list: Node, src: &[u8]) -> Args {
    let mut out = Args::default();
    for a in named_children(list) {
        let v = if a.kind() == "value_argument" {
            named_children(a).last().copied()
        } else {
            Some(a)
        };
        out.positional.push(v.and_then(|v| value(v, src)));
    }
    out
}
