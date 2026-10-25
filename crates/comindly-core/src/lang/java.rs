//! Java syntax adapter (tree-sitter-java). Packages come from the `package` declaration.

use tree_sitter::Node;

use super::{field_text, line, named_children, spring, text, LangSpec, RawContract, Target};
use crate::facts::{Import, ImportItem, RefKind, TypeSource, ValueExpr};
use crate::model::SymbolKind;

pub struct Java;

impl LangSpec for Java {
    fn receiver(&self) -> &'static str {
        "this"
    }

    fn package(&self, root: Node, src: &[u8]) -> Option<String> {
        let decl = named_children(root)
            .into_iter()
            .find(|n| n.kind() == "package_declaration")?;
        named_children(decl)
            .into_iter()
            .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"))
            .map(|n| text(n, src).to_string())
    }

    fn def(&self, node: Node, src: &[u8], _in_class: bool) -> Option<(SymbolKind, String)> {
        let kind = match node.kind() {
            "class_declaration" | "record_declaration" => SymbolKind::Class,
            "interface_declaration" | "annotation_type_declaration" => SymbolKind::Interface,
            "enum_declaration" => SymbolKind::Enum,
            "method_declaration" => SymbolKind::Method,
            "enum_constant" => SymbolKind::Constant,
            _ => return None,
        };
        Some((kind, field_text(node, "name", src)?.to_string()))
    }

    fn variable(&self, _node: Node, _src: &[u8]) -> Option<String> {
        None
    }

    fn contract(&self, node: Node, src: &[u8]) -> Option<RawContract> {
        if let Some(r) = spring::route(node, src) {
            return Some(r);
        }
        if node.kind() != "method_invocation" {
            return None;
        }
        let method = field_text(node, "name", src)?;
        if method == "uri" {
            let inner = node
                .child_by_field_name("object")
                .filter(|o| o.kind() == "method_invocation")?;
            let verb = spring::webclient_verb(field_text(inner, "name", src)?)?;
            return Some(RawContract::Op {
                receiver: vec![spring::WEBCLIENT.into()],
                method: verb,
                args: spring::args(node.child_by_field_name("arguments")?, src),
            });
        }
        if !spring::REST_TEMPLATE.iter().any(|(m, _)| *m == method) {
            return None;
        }
        Some(RawContract::Op {
            receiver: node
                .child_by_field_name("object")
                .and_then(|o| path(o, src))
                .unwrap_or_default(),
            method: method.to_string(),
            args: spring::args(node.child_by_field_name("arguments")?, src),
        })
    }

    fn value(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        if !matches!(
            node.kind(),
            "field_declaration" | "local_variable_declaration" | "constant_declaration"
        ) {
            return None;
        }
        let d = node.child_by_field_name("declarator")?;
        Some((
            field_text(d, "name", src)?.to_string(),
            spring::value(d.child_by_field_name("value")?, src)?,
        ))
    }

    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>> {
        if node.kind() != "import_declaration" {
            return None;
        }
        let kids = named_children(node);
        let full = kids
            .iter()
            .find(|n| matches!(n.kind(), "scoped_identifier" | "identifier"))
            .map(|n| text(*n, src).to_string())?;
        let all = kids.iter().any(|n| n.kind() == "asterisk");
        let (module, items) = if all {
            (full, vec![ImportItem::All])
        } else {
            let (m, name) = full.rsplit_once('.')?;
            (
                m.to_string(),
                vec![ImportItem::Name {
                    name: name.to_string(),
                    alias: None,
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
        match node.kind() {
            "method_invocation" => {
                let name = field_text(node, "name", src)?.to_string();
                let mut p = match node.child_by_field_name("object") {
                    Some(o) => path(o, src)?,
                    None => Vec::new(),
                };
                p.push(name);
                Some((RefKind::Call, p))
            }
            "object_creation_expression" => Some((
                RefKind::New,
                type_path(node.child_by_field_name("type")?, src)?,
            )),
            _ => None,
        }
    }

    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)> {
        let local = match node.kind() {
            "field_declaration" => false,
            "local_variable_declaration" => true,
            _ => return None,
        };
        let decl = node.child_by_field_name("declarator")?;
        let name = field_text(decl, "name", src)?.to_string();
        let target = if local {
            Target::Var(name)
        } else {
            Target::Attr(name)
        };
        let ty = node.child_by_field_name("type")?;
        if text(ty, src) != "var" {
            return Some((target, TypeSource::Annotation(type_path(ty, src)?)));
        }
        let v = decl.child_by_field_name("value")?;
        let source = match v.kind() {
            "object_creation_expression" => {
                TypeSource::Construct(type_path(v.child_by_field_name("type")?, src)?)
            }
            "method_invocation" => TypeSource::Construct(self.reference(v, src)?.1),
            _ => return None,
        };
        Some((target, source))
    }

    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)> {
        let mut out = Vec::new();
        let types = |n: Node| -> Vec<Vec<String>> {
            let list = named_children(n)
                .into_iter()
                .find(|c| c.kind() == "type_list")
                .map(named_children)
                .unwrap_or_else(|| named_children(n));
            list.into_iter().filter_map(|t| type_path(t, src)).collect()
        };
        for c in named_children(def) {
            let kind = match c.kind() {
                "superclass" | "extends_interfaces" => RefKind::Inherits,
                "super_interfaces" => RefKind::Implements,
                _ => continue,
            };
            out.extend(types(c).into_iter().map(|p| (kind, p)));
        }
        out
    }

    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>> {
        type_path(def.child_by_field_name("type")?, src)
    }

    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)> {
        let Some(ps) = def.child_by_field_name("parameters") else {
            return Vec::new();
        };
        named_children(ps)
            .into_iter()
            .filter(|p| p.kind() == "formal_parameter")
            .filter_map(|p| {
                Some((
                    field_text(p, "name", src)?.to_string(),
                    type_path(p.child_by_field_name("type")?, src)?,
                    false,
                ))
            })
            .collect()
    }
}

fn path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" | "type_identifier" => Some(vec![text(node, src).to_string()]),
        "this" => Some(vec!["this".into()]),
        "field_access" => {
            let mut p = path(node.child_by_field_name("object")?, src)?;
            p.push(field_text(node, "field", src)?.to_string());
            Some(p)
        }
        "scoped_identifier" => {
            let mut p = path(node.child_by_field_name("scope")?, src)?;
            p.push(field_text(node, "name", src)?.to_string());
            Some(p)
        }
        _ => None,
    }
}

fn type_path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "type_identifier" | "identifier" => Some(vec![text(node, src).to_string()]),
        "scoped_type_identifier" => {
            let mut out = Vec::new();
            for c in named_children(node) {
                out.extend(type_path(c, src)?);
            }
            (!out.is_empty()).then_some(out)
        }
        "generic_type" => type_path(named_children(node).into_iter().next()?, src),
        _ => None,
    }
}
