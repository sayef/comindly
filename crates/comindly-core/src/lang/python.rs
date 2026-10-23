//! Python syntax adapter (tree-sitter-python).

use tree_sitter::Node;

use super::{field_text, line, named_children, text, LangSpec, RawContract, Target};
use crate::facts::{Args, Import, ImportItem, RefKind, TemplatePart, TypeSource, ValueExpr};
use crate::model::SymbolKind;

pub struct Python;

impl LangSpec for Python {
    fn receiver(&self) -> &'static str {
        "self"
    }

    fn def(&self, node: Node, src: &[u8], in_class: bool) -> Option<(SymbolKind, String)> {
        let kind = match node.kind() {
            "function_definition" if in_class => SymbolKind::Method,
            "function_definition" => SymbolKind::Function,
            "class_definition" => SymbolKind::Class,
            _ => return None,
        };
        Some((kind, field_text(node, "name", src)?.to_string()))
    }

    fn contract(&self, node: Node, src: &[u8]) -> Option<RawContract> {
        self.contract_at(node, src)
    }

    fn value(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        self.value_at(node, src)
    }

    fn variable(&self, node: Node, src: &[u8]) -> Option<String> {
        let left = (node.kind() == "assignment").then(|| node.child_by_field_name("left"))??;
        (left.kind() == "identifier").then(|| text(left, src).to_string())
    }

    fn import(&self, node: Node, src: &[u8]) -> Option<Vec<Import>> {
        match node.kind() {
            // import a.b, c as d — one Import per module so each keeps its alias
            "import_statement" => {
                let mut cur = node.walk();
                let imports = node
                    .children_by_field_name("name", &mut cur)
                    .filter_map(|n| module_item(n, src))
                    .map(|(module, alias)| Import {
                        module,
                        level: 0,
                        items: vec![ImportItem::Module { alias }],
                        line: line(node),
                        file: None,
                    })
                    .collect();
                Some(imports)
            }
            // from [..]m import a, b as c / *
            "import_from_statement" => {
                let m = node.child_by_field_name("module_name")?;
                let (module, level) = if m.kind() == "relative_import" {
                    let mut level = 0;
                    let mut module = String::new();
                    for c in named_children(m) {
                        match c.kind() {
                            "import_prefix" => level = text(c, src).len() as u32,
                            "dotted_name" => module = text(c, src).to_string(),
                            _ => {}
                        }
                    }
                    (module, level)
                } else {
                    (text(m, src).to_string(), 0)
                };
                let mut items = Vec::new();
                let mut cur = node.walk();
                for n in node.children_by_field_name("name", &mut cur) {
                    if let Some((name, alias)) = module_item(n, src) {
                        items.push(ImportItem::Name { name, alias });
                    }
                }
                if named_children(node)
                    .iter()
                    .any(|c| c.kind() == "wildcard_import")
                {
                    items.push(ImportItem::All);
                }
                Some(vec![Import {
                    module,
                    level,
                    items,
                    line: line(node),
                    file: None,
                }])
            }
            _ => None,
        }
    }

    fn reference(&self, node: Node, src: &[u8]) -> Option<(RefKind, Vec<String>)> {
        if node.kind() != "call" {
            return None;
        }
        Some((
            RefKind::Call,
            path(node.child_by_field_name("function")?, src)?,
        ))
    }

    fn assignment(&self, node: Node, src: &[u8]) -> Option<(Target, TypeSource)> {
        if node.kind() != "assignment" {
            return None;
        }
        let left = node.child_by_field_name("left")?;
        let target = match left.kind() {
            "identifier" => Target::Var(text(left, src).to_string()),
            "attribute"
                if left
                    .child_by_field_name("object")
                    .is_some_and(|o| text(o, src) == "self") =>
            {
                Target::Attr(field_text(left, "attribute", src)?.to_string())
            }
            _ => return None,
        };
        let source = if let Some(t) = node.child_by_field_name("type") {
            TypeSource::Annotation(type_path(t, src)?)
        } else {
            let right = node.child_by_field_name("right")?;
            match right.kind() {
                "call" => TypeSource::Construct(path(right.child_by_field_name("function")?, src)?),
                "identifier" => TypeSource::Alias {
                    name: text(right, src).to_string(),
                    scope: String::new(),
                },
                _ => return None,
            }
        };
        Some((target, source))
    }

    fn bases(&self, def: Node, src: &[u8]) -> Vec<(RefKind, Vec<String>)> {
        def.child_by_field_name("superclasses")
            .map(|a| {
                named_children(a)
                    .into_iter()
                    .filter_map(|b| path(b, src))
                    .map(|p| (RefKind::Inherits, p))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn returns(&self, def: Node, src: &[u8]) -> Option<Vec<String>> {
        type_path(def.child_by_field_name("return_type")?, src)
    }

    fn param_names(&self, def: Node, src: &[u8]) -> Vec<String> {
        let Some(ps) = def.child_by_field_name("parameters") else {
            return Vec::new();
        };
        let mut names: Vec<String> = named_children(ps)
            .into_iter()
            .filter_map(|p| match p.kind() {
                "identifier" => Some(text(p, src).to_string()),
                // Everything after `*args` or a bare `*` is keyword-only.
                "list_splat_pattern" | "keyword_separator" => Some("*".to_string()),
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => p
                    .child_by_field_name("name")
                    .or_else(|| {
                        named_children(p)
                            .into_iter()
                            .find(|c| c.kind() == "identifier")
                    })
                    .map(|n| text(n, src).to_string()),
                _ => None,
            })
            .collect();
        if names.first().is_some_and(|n| n == "self" || n == "cls") {
            names.remove(0);
        }
        names
    }

    fn call(&self, node: Node, src: &[u8]) -> Option<(Vec<String>, Args)> {
        if node.kind() != "call" {
            return None;
        }
        let callee = path(node.child_by_field_name("function")?, src)?;
        let a = args(node.child_by_field_name("arguments")?, src);
        (a.positional.iter().any(Option::is_some) || !a.keywords.is_empty()).then_some((callee, a))
    }

    fn params(&self, def: Node, src: &[u8]) -> Vec<(String, Vec<String>, bool)> {
        let Some(ps) = def.child_by_field_name("parameters") else {
            return Vec::new();
        };
        named_children(ps)
            .into_iter()
            .filter(|p| matches!(p.kind(), "typed_parameter" | "typed_default_parameter"))
            .filter_map(|p| {
                let name = p.child_by_field_name("name").or_else(|| {
                    named_children(p)
                        .into_iter()
                        .find(|c| c.kind() == "identifier")
                })?;
                let ty = type_path(p.child_by_field_name("type")?, src)?;
                Some((text(name, src).to_string(), ty, false))
            })
            .collect()
    }
}

/// `a.b` / `a.b as c` → (dotted module or name, alias).
fn module_item(n: Node, src: &[u8]) -> Option<(String, Option<String>)> {
    match n.kind() {
        "aliased_import" => Some((
            field_text(n, "name", src)?.to_string(),
            field_text(n, "alias", src).map(str::to_string),
        )),
        "dotted_name" => Some((text(n, src).to_string(), None)),
        _ => None,
    }
}

/// Dotted path of an identifier/attribute chain; `None` for anything dynamic.
fn path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" => Some(vec![text(node, src).to_string()]),
        "attribute" => {
            let mut p = path(node.child_by_field_name("object")?, src)?;
            p.push(field_text(node, "attribute", src)?.to_string());
            Some(p)
        }
        _ => None,
    }
}

/// Path of a type annotation, unwrapping `Optional[X]`, `Annotated[X, …]` and `X | None`.
fn type_path(node: Node, src: &[u8]) -> Option<Vec<String>> {
    match node.kind() {
        "type" => type_path(named_children(node).into_iter().next()?, src),
        "generic_type" | "subscript" => {
            let head = named_children(node).into_iter().next()?;
            let name = text(head, src);
            if matches!(name, "Optional" | "Annotated" | "typing.Optional") {
                let arg = named_children(node).into_iter().nth(1)?;
                type_path(arg, src)
            } else {
                type_path(head, src)
            }
        }
        "type_parameter" => type_path(named_children(node).into_iter().next()?, src),
        "binary_operator" => type_path(node.child_by_field_name("left")?, src),
        // Forward reference: `-> "Client"`.
        "string" => {
            let t = text(node, src).trim_matches(|c| c == '"' || c == '\'');
            (!t.is_empty()
                && t.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.'))
            .then(|| t.split('.').map(str::to_string).collect())
        }
        _ => path(node, src),
    }
}

// ---- contracts ----------------------------------------------------------------------

const ROUTE_METHODS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];

/// Constructors whose result can carry contract meaning (routers, apps, clients).
const CTORS: &[&str] = &[
    "APIRouter",
    "FastAPI",
    "Client",
    "AsyncClient",
    "Session",
    "ClientSession",
    "client",
    "resource",
];

/// Calls that may be service operations; receivers decide at resolve time.
const OPS: &[&str] = &[
    "get",
    "post",
    "put",
    "patch",
    "delete",
    "head",
    "options",
    "request",
    "stream",
    "send_message",
    "send_message_batch",
    "receive_message",
    "publish",
    "publish_batch",
    "put_object",
    "get_object",
    "upload_file",
    "upload_fileobj",
    "download_file",
    "download_fileobj",
    "head_object",
    "delete_object",
    "list_objects_v2",
    "copy_object",
    "include_router",
    "from_",
    "into",
    "join",
    "query",
    "add",
    "merge",
    "execute",
    "select",
    "insert",
    "update",
    "text",
];

impl Python {
    pub(crate) fn contract_at(&self, node: Node, src: &[u8]) -> Option<RawContract> {
        match node.kind() {
            "decorated_definition" => {
                let def = node.child_by_field_name("definition")?;
                let handler = field_text(def, "name", src)?.to_string();
                named_children(node)
                    .into_iter()
                    .filter(|d| d.kind() == "decorator")
                    .find_map(|d| {
                        let call = named_children(d).into_iter().find(|c| c.kind() == "call")?;
                        let f = call.child_by_field_name("function")?;
                        if f.kind() != "attribute" {
                            return None;
                        }
                        let method = field_text(f, "attribute", src)?;
                        if !ROUTE_METHODS.contains(&method) {
                            return None;
                        }
                        let args = args(call.child_by_field_name("arguments")?, src);
                        Some(RawContract::Route {
                            handler: handler.clone(),
                            receiver: path(f.child_by_field_name("object")?, src)?,
                            method: method.to_uppercase(),
                            path: args.get("path", 0)?.clone(),
                            client: false,
                        })
                    })
            }
            "assignment" => {
                let left = node.child_by_field_name("left")?;
                let right = node.child_by_field_name("right")?;
                if left.kind() == "identifier" && text(left, src) == "__tablename__" {
                    if let Some(ValueExpr::Lit(t)) = value_expr(right, src) {
                        return Some(RawContract::TableName(t));
                    }
                }
                if right.kind() != "call" {
                    return None;
                }
                let ctor = path(right.child_by_field_name("function")?, src)?;
                if !CTORS.contains(&ctor.last()?.as_str()) {
                    return None;
                }
                let (target, _) = self.assignment(node, src)?;
                Some(RawContract::Construct {
                    target,
                    ctor,
                    args: args(right.child_by_field_name("arguments")?, src),
                })
            }
            "call" => {
                let f = node.child_by_field_name("function")?;
                let (receiver, method) = match f.kind() {
                    "identifier" => (Vec::new(), text(f, src).to_string()),
                    "attribute" => (
                        path(f.child_by_field_name("object")?, src).unwrap_or_default(),
                        field_text(f, "attribute", src)?.to_string(),
                    ),
                    _ => return None,
                };
                if !OPS.contains(&method.as_str()) {
                    return None;
                }
                Some(RawContract::Op {
                    receiver,
                    method,
                    args: args(node.child_by_field_name("arguments")?, src),
                })
            }
            _ => None,
        }
    }

    /// `name = v`, or `self.name = v` (returned as `self.name`; the walk files it on the class).
    pub(crate) fn value_at(&self, node: Node, src: &[u8]) -> Option<(String, ValueExpr)> {
        if node.kind() != "assignment" {
            return None;
        }
        let left = node.child_by_field_name("left")?;
        let name = match left.kind() {
            "identifier" => text(left, src).to_string(),
            "attribute"
                if left
                    .child_by_field_name("object")
                    .is_some_and(|o| text(o, src) == "self") =>
            {
                format!("self.{}", field_text(left, "attribute", src)?)
            }
            _ => return None,
        };
        let v = value_expr(node.child_by_field_name("right")?, src)?;
        Some((name, v))
    }
}

fn args(list: Node, src: &[u8]) -> Args {
    let mut out = Args::default();
    for a in named_children(list) {
        match a.kind() {
            "keyword_argument" => {
                if let (Some(k), Some(v)) = (
                    field_text(a, "name", src),
                    a.child_by_field_name("value")
                        .and_then(|v| value_expr(v, src)),
                ) {
                    out.keywords.push((k.to_string(), v));
                }
            }
            // `**params`: kept so keyword lookups can see into the dict it names.
            "dictionary_splat" => {
                if let Some(v) = named_children(a).first().and_then(|v| value_expr(*v, src)) {
                    out.keywords.push(("**".into(), v));
                }
            }
            "list_splat" | "comment" => {}
            _ => {
                if a.kind() == "call" {
                    if let Some(p) = a.child_by_field_name("function").and_then(|f| path(f, src)) {
                        out.calls.push((out.positional.len(), p));
                    }
                }
                out.positional.push(value_expr(a, src));
            }
        }
    }
    out
}

/// What an expression evaluates to statically: literal, template, env var, or a name.
fn value_expr(node: Node, src: &[u8]) -> Option<ValueExpr> {
    match node.kind() {
        // `a if cond else b`: the value when the condition holds.
        "conditional_expression" => value_expr(*named_children(node).first()?, src),
        "await" => value_expr(*named_children(node).first()?, src),
        "string" => {
            let mut parts = Vec::new();
            for c in named_children(node) {
                match c.kind() {
                    "string_content" => parts.push(TemplatePart::Lit(text(c, src).to_string())),
                    "interpolation" => parts.push(TemplatePart::Expr(
                        c.child_by_field_name("expression")
                            .and_then(|e| path(e, src)),
                    )),
                    _ => {}
                }
            }
            if parts.iter().all(|p| matches!(p, TemplatePart::Lit(_))) {
                let s: String = parts
                    .into_iter()
                    .map(|p| match p {
                        TemplatePart::Lit(s) => s,
                        TemplatePart::Expr(_) => String::new(),
                    })
                    .collect();
                Some(ValueExpr::Lit(s))
            } else {
                Some(ValueExpr::Template(parts))
            }
        }
        "concatenated_string" => {
            let mut parts = Vec::new();
            for c in named_children(node) {
                match value_expr(c, src)? {
                    ValueExpr::Lit(s) => parts.push(TemplatePart::Lit(s)),
                    ValueExpr::Template(p) => parts.extend(p),
                    _ => return None,
                }
            }
            Some(ValueExpr::Template(parts))
        }
        // os.environ["X"]
        "subscript" => {
            let value = node.child_by_field_name("value")?;
            if path(value, src)?.join(".") != "os.environ" {
                return None;
            }
            match value_expr(node.child_by_field_name("subscript")?, src)? {
                ValueExpr::Lit(k) => Some(ValueExpr::Env(k)),
                _ => None,
            }
        }
        // os.getenv("X", …) / os.environ.get("X", …); other calls kept for known callees.
        "call" => {
            let f = path(node.child_by_field_name("function")?, src)?.join(".");
            // `getattr(obj, "x")`, also through `asyncio.to_thread(getattr, obj, "x")`.
            let list = named_children(node.child_by_field_name("arguments")?);
            let attr = match (f.as_str(), list.as_slice()) {
                ("getattr", [obj, name, ..]) => Some((*obj, *name)),
                (t, [g, obj, name, ..])
                    if t.ends_with("to_thread") && text(*g, src) == "getattr" =>
                {
                    Some((*obj, *name))
                }
                _ => None,
            };
            if let Some((obj, name)) = attr {
                let (Some(mut p), Some(ValueExpr::Lit(n))) =
                    (path(obj, src), value_expr(name, src))
                else {
                    return None;
                };
                p.push(n);
                return Some(ValueExpr::Name(p));
            }
            if !matches!(
                f.as_str(),
                "os.getenv" | "getenv" | "os.environ.get" | "environ.get"
            ) {
                return call_value(node, src);
            }
            let first = named_children(node.child_by_field_name("arguments")?)
                .into_iter()
                .next()?;
            match value_expr(first, src)? {
                ValueExpr::Lit(k) => Some(ValueExpr::Env(k)),
                _ => None,
            }
        }
        "identifier" | "attribute" => path(node, src).map(ValueExpr::Name),
        "binary_operator" => {
            let sep = match field_text(node, "operator", src)? {
                "/" => "/",
                "+" => "",
                _ => return None,
            };
            let mut parts = Vec::new();
            for side in ["left", "right"] {
                match value_expr(node.child_by_field_name(side)?, src)? {
                    ValueExpr::Join { sep: s, parts: p } if s == sep => parts.extend(p),
                    v => parts.push(v),
                }
            }
            Some(ValueExpr::Join {
                sep: sep.to_string(),
                parts,
            })
        }
        "dictionary" => Some(ValueExpr::Record(
            named_children(node)
                .into_iter()
                .filter(|p| p.kind() == "pair")
                .filter_map(|p| {
                    let ValueExpr::Lit(k) = value_expr(p.child_by_field_name("key")?, src)? else {
                        return None;
                    };
                    Some((k, value_expr(p.child_by_field_name("value")?, src)?))
                })
                .collect(),
        )),
        "parenthesized_expression" => value_expr(named_children(node).into_iter().next()?, src),
        _ => None,
    }
}

/// A call as a value: positional/keyword argument values by callee path.
fn call_value(node: Node, src: &[u8]) -> Option<ValueExpr> {
    let func = path(node.child_by_field_name("function")?, src)?;
    let a = args(node.child_by_field_name("arguments")?, src);
    Some(ValueExpr::Call {
        func,
        args: a.positional.into_iter().map_while(|v| v).collect(),
        kwargs: a.keywords,
    })
}
