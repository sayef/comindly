//! Kotlin syntax adapter (tree-sitter-kotlin-ng). Shares the JVM package namespace with Java.

use tree_sitter::Node;

use super::{field_text, line, named_children, spring, text, LangSpec, RawContract, Target};
use crate::facts::{Import, ImportItem, RefKind, TypeSource, ValueExpr};
use crate::model::SymbolKind;

pub struct Kotlin;

impl LangSpec for Kotlin {
    fn receiver(&self) -> &'static str {
        "this"
    }

    fn contract(&self, node: Node, src: &[u8]) -> Option<RawContract> {
        if let Some(r) = spring::route(node, src) {
            return Some(r);
        }
        if node.kind() != "call_expression" {
            return None;
        }
        let kids = named_children(node);
        let callee = kids
            .first()
            .filter(|k| k.kind() == "navigation_expression")?;
        let parts = named_children(*callee);
        if parts.last().is_some_and(|l| text(*l, src) == "uri") {
            let inner = parts.first().filter(|p| p.kind() == "call_expression")?;
            let inner_nav = named_children(*inner)
                .into_iter()
                .find(|k| k.kind() == "navigation_expression")?;
            let verb = spring::webclient_verb(text(*named_children(inner_nav).last()?, src))?;
            let list = kids.iter().find(|k| k.kind() == "value_arguments")?;
            return Some(RawContract::Op {
                receiver: vec![spring::WEBCLIENT.into()],
                method: verb,
                args: spring::args(*list, src),
            });
        }
        let mut receiver = path(*callee, src)?;
        let method = receiver.pop()?;
        if !spring::REST_TEMPLATE.iter().any(|(m, _)| *m == method) {
            return None;
        }
        let list = kids.iter().find(|k| k.kind() == "value_arguments")?;
        Some(RawContract::Op {
            receiver,
            method,
            args: spring::args(*list, src),
        })
    }

    fn value(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        if node.kind() != "property_declaration" {
            return None;
        }
        let name = property_name(node, src)?;
        let init = named_children(node).into_iter().last()?;
        Some((name, spring::value(init, src)?))
    }

    fn package(&self, root: Node, src: &[u8]) -> Option<String> {
        let header = named_children(root)
            .into_iter()
            .find(|n| n.kind() == "package_header")?;
        named_children(header)
            .into_iter()
            .find(|n| n.kind() == "qualified_identifier")
            .map(|n| text(n, src).to_string())
    }

    fn def(&self, node: Node, src: &[u8], in_class: bool) -> Option<(SymbolKind, String)> {
        let kind = match node.kind() {
            "class_declaration" if has_token(node, "interface") => SymbolKind::Interface,
            "class_declaration" | "object_declaration" => SymbolKind::Class,
            "function_declaration" if in_class => SymbolKind::Method,
            "function_declaration" => SymbolKind::Function,
            "enum_entry" => {
                let name = named_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "identifier")?;
                return Some((SymbolKind::Constant, text(name, src).to_string()));
            }
            // `@A @B(x) public annotation class Name` (no body) misparses as an expression.
            "infix_expression" => {
                let ids: Vec<&str> = named_children(node)
                    .into_iter()
                    .filter(|n| n.kind() == "identifier")
                    .map(|n| text(n, src))
                    .collect();
                let at = ids.iter().position(|t| *t == "class")?;
                return Some((SymbolKind::Class, ids.get(at + 1)?.to_string()));
            }
            // The grammar wraps some valid classes in ERROR; recover `class Name …`.
            "ERROR" if has_token(node, "class") || has_token(node, "object") => {
                let name = named_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "identifier")?;
                return Some((SymbolKind::Class, text(name, src).to_string()));
            }
            _ => return None,
        };
        Some((kind, field_text(node, "name", src)?.to_string()))
    }

    fn variable(&self, node: Node, src: &[u8]) -> Option<String> {
        (node.kind() == "property_declaration").then(|| property_name(node, src))?
    }

    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>> {
        if node.kind() != "import" {
            return None;
        }
        let kids = named_children(node);
        let full = kids
            .iter()
            .find(|n| n.kind() == "qualified_identifier")
            .map(|n| text(*n, src).to_string())?;
        let alias = kids
            .iter()
            .rev()
            .find(|n| n.kind() == "identifier")
            .map(|n| text(*n, src).to_string());
        let (module, items) = if has_token(node, "*") {
            (full, vec![ImportItem::All])
        } else {
            let (m, name) = full.rsplit_once('.')?;
            (
                m.to_string(),
                vec![ImportItem::Name {
                    name: name.to_string(),
                    alias,
                }],
            )
        };
        Some(vec![Import {
            module,
            level: 0,
            items,
            line: line(node),
            file: None,
        }])
    }

    fn reference(&self, node: Node, src: &[u8]) -> Option<(RefKind, Vec<String>)> {
        if node.kind() != "call_expression" {
            return None;
        }
        Some((
            RefKind::Call,
            path(named_children(node).into_iter().next()?, src)?,
        ))
    }

    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)> {
        match node.kind() {
            "property_declaration" => {
                let name = property_name(node, src)?;
                let in_class = node.parent().is_some_and(|p| p.kind() == "class_body");
                let target = if in_class {
                    Target::Attr(name)
                } else {
                    Target::Var(name)
                };
                let decl = named_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "variable_declaration")?;
                if let Some(t) = named_children(decl)
                    .into_iter()
                    .find_map(|t| type_path(t, src))
                {
                    return Some((target, TypeSource::Annotation(t)));
                }
                let value = named_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "call_expression")?;
                let callee = path(named_children(value).into_iter().next()?, src)?;
                Some((target, TypeSource::Construct(callee)))
            }
            // `class S(private val repo: Repo)` declares a property.
            "class_parameter" if has_token(node, "val") || has_token(node, "var") => {
                let kids = named_children(node);
                let name = kids.iter().find(|n| n.kind() == "identifier")?;
                let ty = kids.iter().find_map(|t| type_path(*t, src))?;
                Some((
                    Target::Attr(text(*name, src).to_string()),
                    TypeSource::Annotation(ty),
                ))
            }
            _ => None,
        }
    }

    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)> {
        let Some(specs) = named_children(def)
            .into_iter()
            .find(|n| n.kind() == "delegation_specifiers")
        else {
            return Vec::new();
        };
        named_children(specs)
            .into_iter()
            .filter_map(|spec| {
                let inner = named_children(spec).into_iter().next()?;
                match inner.kind() {
                    "constructor_invocation" => Some((
                        RefKind::Inherits,
                        type_path(named_children(inner).into_iter().next()?, src)?,
                    )),
                    _ => Some((RefKind::Implements, type_path(inner, src)?)),
                }
            })
            .collect()
    }

    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>> {
        let kids = named_children(def);
        let params = kids
            .iter()
            .position(|n| n.kind() == "function_value_parameters")?;
        kids[params + 1..]
            .iter()
            .take_while(|n| n.kind() != "function_body")
            .find_map(|t| type_path(*t, src))
    }

    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)> {
        let Some(ps) = named_children(def)
            .into_iter()
            .find(|n| n.kind() == "function_value_parameters")
        else {
            return Vec::new();
        };
        named_children(ps)
            .into_iter()
            .filter(|p| p.kind() == "parameter")
            .filter_map(|p| {
                let kids = named_children(p);
                let name = kids.iter().find(|n| n.kind() == "identifier")?;
                let ty = kids.iter().find_map(|t| type_path(*t, src))?;
                Some((text(*name, src).to_string(), ty, false))
            })
            .collect()
    }
}

fn has_token(node: Node, kind: &str) -> bool {
    (0..node.child_count())
        .filter_map(|i| node.child(i))
        .any(|c| c.kind() == kind)
}

fn property_name(node: Node, src: &[u8]) -> Option<String> {
    let decl = named_children(node)
        .into_iter()
        .find(|n| n.kind() == "variable_declaration")?;
    named_children(decl)
        .into_iter()
        .find(|n| n.kind() == "identifier")
        .map(|n| text(n, src).to_string())
}

fn path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" => Some(vec![text(node, src).to_string()]),
        "this_expression" => Some(vec!["this".into()]),
        "navigation_expression" => {
            let kids = named_children(node);
            let mut p = path(*kids.first()?, src)?;
            let last = kids.last()?;
            if last.kind() != "identifier" {
                return None;
            }
            p.push(text(*last, src).to_string());
            Some(p)
        }
        _ => None,
    }
}

fn type_path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "user_type" => {
            let ids: Vec<String> = named_children(node)
                .into_iter()
                .filter(|n| n.kind() == "identifier")
                .map(|n| text(n, src).to_string())
                .collect();
            (!ids.is_empty()).then_some(ids)
        }
        "nullable_type" | "type" | "parenthesized_type" => named_children(node)
            .into_iter()
            .find_map(|t| type_path(t, src)),
        _ => None,
    }
}
