//! Terraform: queues, topics and buckets a repo declares or looks up, and which queues
//! subscribe to which topics. Becomes contract facts on the `.tf` file's symbol.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use hcl::expr::{Expression, TemplateExpr, TraversalOperator};
use hcl::structure::{Block, Body};
use hcl::template::Element;
use ignore::WalkBuilder;

use crate::facts::{ContractFact, FileFacts, InfraRef};
use crate::model::{GlobalSymbolId, Language, Position, Range, RepoId, Symbol, SymbolKind};

/// Resource types → (scheme, attribute holding the name).
const RESOURCES: &[(&str, &str, &str)] = &[
    ("aws_sqs_queue", "sqs", "name"),
    ("aws_sns_topic", "sns", "name"),
    ("aws_s3_bucket", "s3", "bucket"),
];

/// Module inputs that name the resource a module creates (team modules hide the full name).
const MODULE_NAMES: &[(&str, &str)] = &[
    ("queue_name", "sqs"),
    ("topic_name", "sns"),
    ("bucket_name", "s3"),
];

pub fn is_tf(rel: &str) -> bool {
    rel.ends_with(".tf") && !rel.split('/').any(|d| d == ".terraform")
}

/// Symbols and facts for every `.tf` file in a repo.
pub fn scan(root: &Path, repo: &str) -> Vec<(Symbol, FileFacts)> {
    let rels: Vec<String> = WalkBuilder::new(root)
        .hidden(false)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .filter(|r| is_tf(r))
        .collect();
    files(root, repo, &rels)
}

/// Facts for these `.tf` files; locals and references resolve across their module directory.
pub fn files(root: &Path, repo: &str, rels: &[String]) -> Vec<(Symbol, FileFacts)> {
    let mut dirs: BTreeMap<String, Vec<&String>> = BTreeMap::new();
    for r in rels.iter().filter(|r| is_tf(r)) {
        let dir = r.rsplit_once('/').map_or("", |(d, _)| d).to_string();
        dirs.entry(dir).or_default().push(r);
    }
    let mut out = Vec::new();
    for (dir, wanted) in dirs {
        let module = read_dir(root, &dir);
        for rel in wanted {
            let Some(body) = module.bodies.get(rel.as_str()) else {
                continue;
            };
            let contracts = module.facts(body);
            out.push(file(repo, rel, contracts));
        }
    }
    out
}

struct Module {
    bodies: BTreeMap<String, Body>,
    locals: HashMap<String, Expression>,
    /// `aws_sqs_queue.x`, `data.aws_sns_topic.y`, `module.z` → what it names.
    handles: HashMap<String, InfraRef>,
}

fn read_dir(root: &Path, dir: &str) -> Module {
    let mut m = Module {
        bodies: BTreeMap::new(),
        locals: HashMap::new(),
        handles: HashMap::new(),
    };
    let full = if dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(dir)
    };
    for e in std::fs::read_dir(full).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.ends_with(".tf") {
            continue;
        }
        let rel = if dir.is_empty() {
            name
        } else {
            format!("{dir}/{name}")
        };
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        if let Ok(body) = hcl::parse(&text) {
            m.bodies.insert(rel, body);
        }
    }
    for body in m.bodies.values() {
        for b in body.blocks().filter(|b| b.identifier.as_str() == "locals") {
            for a in b.body.attributes() {
                m.locals.insert(a.key.as_str().to_string(), a.expr.clone());
            }
        }
    }
    let mut handles = HashMap::new();
    for body in m.bodies.values() {
        for b in body.blocks() {
            if let Some((key, r)) = m.handle(b) {
                handles.insert(key, r);
            }
        }
    }
    m.handles = handles;
    m
}

impl Module {
    /// What a `resource`, `data` or `module` block names, keyed by how other blocks refer to it.
    fn handle(&self, b: &Block) -> Option<(String, InfraRef)> {
        let labels: Vec<&str> = b.labels.iter().map(|l| l.as_str()).collect();
        match (b.identifier.as_str(), labels.as_slice()) {
            ("resource", [ty, name]) | ("data", [ty, name]) => {
                let (_, scheme, attr) = RESOURCES.iter().find(|(t, _, _)| t == ty)?;
                let value = self.attr(&b.body, attr)?;
                let key = if b.identifier.as_str() == "data" {
                    format!("data.{ty}.{name}")
                } else {
                    format!("{ty}.{name}")
                };
                Some((
                    key,
                    InfraRef {
                        scheme: scheme.to_string(),
                        name: value,
                        logical: false,
                    },
                ))
            }
            ("module", [name]) => {
                let (attr, scheme) = MODULE_NAMES
                    .iter()
                    .find(|(a, _)| b.body.attributes().any(|x| x.key.as_str() == *a))?;
                Some((
                    format!("module.{name}"),
                    InfraRef {
                        scheme: scheme.to_string(),
                        name: self.attr(&b.body, attr)?,
                        logical: true,
                    },
                ))
            }
            _ => None,
        }
    }

    fn attr(&self, body: &Body, key: &str) -> Option<String> {
        let a = body.attributes().find(|a| a.key.as_str() == key)?;
        self.eval(&a.expr, 0)
    }

    /// A string value: literals, templates, `local.*` (resolved) and `var.*` (unknown → `{}`).
    fn eval(&self, e: &Expression, depth: u32) -> Option<String> {
        if depth > 8 {
            return None;
        }
        match e {
            Expression::String(s) => Some(s.clone()),
            Expression::TemplateExpr(t) => {
                let raw = match t.as_ref() {
                    TemplateExpr::QuotedString(s) => s.clone(),
                    TemplateExpr::Heredoc(h) => h.template.clone(),
                };
                let tpl: hcl::Template = raw.parse().ok()?;
                let mut out = String::new();
                for el in tpl.elements() {
                    match el {
                        Element::Literal(l) => out.push_str(l),
                        Element::Interpolation(i) => out.push_str(
                            &self.eval(&i.expr, depth + 1).unwrap_or_else(|| "{}".into()),
                        ),
                        Element::Directive(_) => return None,
                    }
                }
                Some(out)
            }
            Expression::Traversal(t) => {
                let Expression::Variable(root) = &t.expr else {
                    return None;
                };
                let Some(TraversalOperator::GetAttr(first)) = t.operators.first() else {
                    return None;
                };
                match root.as_str() {
                    "var" => Some("{}".into()),
                    "local" if t.operators.len() == 1 => {
                        self.eval(self.locals.get(first.as_str())?, depth + 1)
                    }
                    _ => None,
                }
            }
            Expression::Parenthesis(p) => self.eval(p, depth + 1),
            _ => None,
        }
    }

    /// The infrastructure a reference expression (`data.aws_sns_topic.x.arn`) points at.
    fn target(&self, e: &Expression) -> Option<InfraRef> {
        let Expression::Traversal(t) = e else {
            return None;
        };
        let Expression::Variable(root) = &t.expr else {
            return None;
        };
        let names: Vec<&str> = t
            .operators
            .iter()
            .map_while(|o| match o {
                TraversalOperator::GetAttr(i) => Some(i.as_str()),
                _ => None,
            })
            .collect();
        let key = match (root.as_str(), names.as_slice()) {
            ("data", [ty, name, ..]) => format!("data.{ty}.{name}"),
            ("module", [name, ..]) => format!("module.{name}"),
            (ty, [name, ..]) => format!("{ty}.{name}"),
            _ => return None,
        };
        self.handles.get(&key).cloned()
    }

    fn facts(&self, body: &Body) -> Vec<ContractFact> {
        let mut out = Vec::new();
        for b in body.blocks() {
            let labels: Vec<&str> = b.labels.iter().map(|l| l.as_str()).collect();
            let own = match b.identifier.as_str() {
                "data" => None,
                _ => self.handle(b).map(|(_, r)| r),
            };
            if let Some(r) = &own {
                out.push(ContractFact::Infra {
                    declares: r.clone(),
                    subscribes: None,
                });
            }
            // `aws_sns_topic_subscription { topic_arn, endpoint }` → queue subscribes to topic.
            if b.identifier.as_str() == "resource"
                && labels.first() == Some(&"aws_sns_topic_subscription")
            {
                let get = |k: &str| b.body.attributes().find(|a| a.key.as_str() == k);
                if let (Some(t), Some(q)) = (
                    get("topic_arn").and_then(|a| self.target(&a.expr)),
                    get("endpoint").and_then(|a| self.target(&a.expr)),
                ) {
                    out.push(ContractFact::Infra {
                        declares: q,
                        subscribes: Some(t),
                    });
                }
            }
            // A queue module wired to topics through any `*topic_arn` input.
            if let Some(q) = own.filter(|r| r.scheme == "sqs") {
                let mut topics = Vec::new();
                for a in b.body.attributes() {
                    self.topic_refs(a.key.as_str(), &a.expr, &mut topics);
                }
                for t in topics {
                    out.push(ContractFact::Infra {
                        declares: q.clone(),
                        subscribes: Some(t),
                    });
                }
            }
        }
        out
    }

    fn topic_refs(&self, key: &str, e: &Expression, out: &mut Vec<InfraRef>) {
        match e {
            Expression::Object(o) => {
                for (k, v) in o {
                    let k = match k {
                        hcl::ObjectKey::Identifier(i) => i.as_str().to_string(),
                        hcl::ObjectKey::Expression(x) => self.eval(x, 0).unwrap_or_default(),
                        _ => String::new(),
                    };
                    self.topic_refs(&k, v, out);
                }
            }
            Expression::Array(a) => a.iter().for_each(|v| self.topic_refs(key, v, out)),
            other if key.ends_with("topic_arn") => {
                out.extend(self.target(other).filter(|t| t.scheme == "sns"))
            }
            _ => {}
        }
    }
}

fn file(repo: &str, rel: &str, contracts: Vec<ContractFact>) -> (Symbol, FileFacts) {
    let id = GlobalSymbolId {
        scheme: "comindly-treesitter".into(),
        package_manager: ".".into(),
        package: repo.to_string(),
        version: ".".into(),
        descriptor: rel.to_string(),
    };
    let at = Position { line: 1, column: 0 };
    let sym = Symbol {
        id: id.clone(),
        name: rel.to_string(),
        kind: SymbolKind::File,
        language: Language::Other("hcl".into()),
        repo: RepoId(repo.to_string()),
        file_path: rel.to_string(),
        range: Range { start: at, end: at },
        signature: None,
        docstring: None,
        fingerprint: None,
    };
    let facts = FileFacts {
        repo: repo.to_string(),
        path: rel.to_string(),
        language: Language::Other("hcl".into()),
        file_id: id.render(),
        modules: Vec::new(),
        imports: Vec::new(),
        refs: Vec::new(),
        types: Vec::new(),
        scip: None,
        contracts,
        values: Vec::new(),
        params: Vec::new(),
        calls: Vec::new(),
    };
    (sym, facts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declarations_and_subscriptions() {
        let dir = std::env::temp_dir().join(format!("comindly-tf-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("iac")).unwrap();
        std::fs::write(
            dir.join("iac/main.tf"),
            r#"
locals { team = "acme" }
data "aws_sns_topic" "upstream" { name = "${local.team}-${var.tier}-orders-events" }
module "events_queue" {
  source     = "git::https://example/modules//queue"
  queue_name = "order-events"
  sns_subscriptions = { orders = { topic_arn = data.aws_sns_topic.upstream.arn } }
}
resource "aws_s3_bucket" "files" { bucket = "acme-files" }
"#,
        )
        .unwrap();
        let out = scan(&dir, "svc");
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(out.len(), 1);
        let c = &out[0].1.contracts;
        let has = |scheme: &str, name: &str, sub: Option<&str>| {
            c.iter().any(|f| match f {
                ContractFact::Infra {
                    declares,
                    subscribes,
                } => {
                    declares.scheme == scheme
                        && declares.name == name
                        && subscribes.as_ref().map(|s| s.name.as_str()) == sub
                }
                _ => false,
            })
        };
        assert!(has("sqs", "order-events", None));
        assert!(has("sqs", "order-events", Some("acme-{}-orders-events")));
        assert!(has("s3", "acme-files", None));
    }
}
