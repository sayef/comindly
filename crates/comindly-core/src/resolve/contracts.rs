//! Contract graph: HTTP routes ↔ clients, queues/topics, buckets and tables, keyed so that
//! providers and consumers meet across repos. Values resolve through constants, settings, env.

use std::collections::HashMap;

use sqlparser::ast::{ObjectName, Statement, TableFactor, TableWithJoins};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use super::{Binder, Value};
use crate::facts::{Args, ContractFact, FileFacts, ImportItem, TemplatePart, ValueExpr};
use crate::model::{
    EdgeKind, GlobalSymbolId, Language, Position, Range, RepoId, Symbol, SymbolKind,
};

const HTTP_VERBS: &[&str] = &["get", "post", "put", "patch", "delete", "head", "options"];
const HTTP_CLIENTS: &[&str] = &[
    "httpx.Client",
    "httpx.AsyncClient",
    "requests.Session",
    "aiohttp.ClientSession",
];
const HTTP_MODULES: &[&str] = &["httpx", "requests", "axios", "ky", "got"];
const ROUTERS: &[&str] = &[
    "fastapi.APIRouter",
    "fastapi.FastAPI",
    "express",
    "express.Router",
];
const DB_READS: &[&str] = &["get", "query", "select", "scalars"];
const DB_WRITES: &[&str] = &["add", "merge", "delete", "insert", "update"];
/// Calls whose value is their first argument's (`Path("/x")`, `str(x)`).
const PASS_THROUGH: &[&str] = &["Path", "PurePath", "PurePosixPath", "str", "URL", "HttpUrl"];
/// Calls building record-like values whose keyword arguments are attributes.
const RECORDS: &[&str] = &["AttrDict", "dict", "SimpleNamespace", "Namespace"];

/// The repo-like owner of contract nodes.
pub const CONTRACTS: &str = "_contracts";

#[derive(Clone)]
enum Client {
    Http,
    Aws(String),
}

pub(super) struct Linked {
    pub symbols: Vec<Symbol>,
    pub edges: Vec<(String, String, EdgeKind)>,
}

pub(super) fn link(b: &Binder, facts: &[FileFacts], calls: &[(String, String)]) -> Linked {
    let mut cx = Cx::new(b, facts);
    cx.performing(facts, calls);
    let mut out = Linked {
        symbols: Vec::new(),
        edges: Vec::new(),
    };
    let mut emit = |src: &str, key: String, kind: EdgeKind| {
        // Contracts describe production topology; tests calling routes or fake buckets are noise.
        if b.file_facts_of(src).is_some_and(|f| test_path(&f.path)) {
            return;
        }
        out.edges
            .push((src.to_string(), contract_id(&key).render(), kind));
        if !out.symbols.iter().any(|s: &Symbol| s.name == key) {
            out.symbols.push(contract_symbol(&key));
        }
    };

    for f in facts {
        for c in &f.contracts {
            match c {
                ContractFact::Table { class, name } => {
                    emit(class, format!("table:{name}"), EdgeKind::Defines)
                }
                ContractFact::Route {
                    handler,
                    receiver,
                    method,
                    path,
                    client,
                    ..
                } => {
                    // Read the path where the handler lives (a class for Spring, else the file).
                    let at = cx.b.parent.get(handler).unwrap_or(&f.file_id);
                    let Some(p) = cx.value(path, at, 0) else {
                        continue;
                    };
                    // File-routed handlers (Next.js) carry their full path and no router.
                    let fulls = if receiver.is_empty() {
                        vec![p]
                    } else {
                        let Some(router) = cx.var_of(receiver, &f.file_id, f) else {
                            continue;
                        };
                        if !cx.routers.contains_key(&router) {
                            continue;
                        }
                        cx.prefixes(&router, 0)
                            .into_iter()
                            .map(|pre| format!("{pre}{p}"))
                            .collect()
                    };
                    let kind = if *client {
                        EdgeKind::Consumes
                    } else {
                        EdgeKind::Serves
                    };
                    for full in fulls {
                        emit(
                            handler,
                            format!("http:{method} {}", http_path(&full)),
                            kind.clone(),
                        );
                    }
                }
                ContractFact::Handle {
                    from,
                    receiver,
                    method,
                    path,
                    handler,
                    ..
                } => {
                    let Some(p) = cx.value(path, from, 0) else {
                        continue;
                    };
                    let p = format!("{}{p}", cx.group_prefix(from, receiver, 0));
                    let src = match cx.b.resolve_path(handler, from, f, 0) {
                        Some(Value::Sym(id)) if !handler.is_empty() => id,
                        _ => from.clone(),
                    };
                    emit(
                        &src,
                        format!("http:{method} {}", http_path(&p)),
                        EdgeKind::Serves,
                    );
                }
                ContractFact::Op {
                    from,
                    receiver,
                    method,
                    args,
                    ..
                } => cx.op(f, from, receiver, method, args, &mut emit),
                ContractFact::Construct { .. } | ContractFact::Infra { .. } => {}
            }
        }
    }
    cx.propagate(facts, &mut emit);
    link_infra(b, facts, &mut out);
    suffix_match(&mut out);
    let mut seen = std::collections::HashSet::new();
    out.edges
        .retain(|(a, b, k)| seen.insert((a.clone(), b.clone(), k.as_str())));
    out
}

/// IaC declarations and topic→queue subscriptions. A module's logical name (`inference-tasks`)
/// joins the one contract of that kind the same repo's code uses whose name ends with it.
fn link_infra(b: &Binder, facts: &[FileFacts], out: &mut Linked) {
    let repo_of = |id: &str| b.file_facts_of(id).map(|f| f.repo.clone());
    let mut used: Vec<(String, String)> = Vec::new(); // (repo, contract name)
    let names: HashMap<String, String> = out
        .symbols
        .iter()
        .map(|s| (s.id.render(), s.name.clone()))
        .collect();
    for (src, dst, _) in &out.edges {
        if let (Some(r), Some(n)) = (repo_of(src), names.get(dst)) {
            used.push((r, n.clone()));
        }
    }
    // Full (non-logical) IaC names are candidates too: another repo may name this one's topic.
    for f in facts {
        for c in &f.contracts {
            if let ContractFact::Infra {
                declares,
                subscribes,
            } = c
            {
                for r in std::iter::once(declares)
                    .chain(subscribes)
                    .filter(|r| !r.logical)
                {
                    used.push((String::new(), format!("{}:{}", r.scheme, untier(&r.name))));
                }
            }
        }
    }
    let key = |r: &crate::facts::InfraRef, repo: &str| -> String {
        let full = format!("{}:{}", r.scheme, untier(&r.name));
        if !r.logical {
            return full;
        }
        let want = name_tokens(&untier(&r.name));
        let own = name_tokens(repo);
        let contains = |have: &[String], part: &[String]| {
            !part.is_empty() && have.windows(part.len()).any(|w| w == part)
        };
        let mut hits: Vec<&String> = used
            .iter()
            .filter(|(_, n)| n.starts_with(&format!("{}:", r.scheme)))
            .filter(|(rr, n)| {
                let have = name_tokens(&n[r.scheme.len() + 1..]);
                let ends = have.len() >= want.len() && have[have.len() - want.len()..] == want[..];
                // Used by this repo's code, or a full name that carries this repo's name.
                ends && (rr == repo || contains(&have, &own))
            })
            .map(|(_, n)| n)
            .collect();
        hits.sort();
        hits.dedup();
        match hits.as_slice() {
            [one] => one.to_string(),
            _ => full,
        }
    };
    let add = |src: String, k: String, kind: EdgeKind, out: &mut Linked| {
        let e = (src, contract_id(&k).render(), kind);
        if !out.edges.contains(&e) {
            out.edges.push(e);
        }
        if !out.symbols.iter().any(|s| s.name == k) {
            out.symbols.push(contract_symbol(&k));
        }
    };
    for f in facts {
        for c in &f.contracts {
            let ContractFact::Infra {
                declares,
                subscribes,
            } = c
            else {
                continue;
            };
            let q = key(declares, &f.repo);
            add(f.file_id.clone(), q.clone(), EdgeKind::Defines, out);
            if let Some(t) = subscribes {
                let t = key(t, &f.repo);
                add(contract_id(&q).render(), t, EdgeKind::Subscribes, out);
            }
        }
    }
}

/// A client sees paths below its configured base URL (often `/api/v1`), so a consumer-only
/// HTTP contract links to the one provider route it is a segment-aligned suffix of.
fn suffix_match(out: &mut Linked) {
    let ids: HashMap<String, String> = out
        .symbols
        .iter()
        .map(|s| (s.id.render(), s.name.clone()))
        .collect();
    let served: Vec<&String> = out
        .edges
        .iter()
        .filter(|(_, _, k)| *k == EdgeKind::Serves)
        .filter_map(|(_, d, _)| ids.get(d))
        .collect();
    let mut redirect: HashMap<String, String> = HashMap::new();
    for (id, name) in &ids {
        let Some((verb, path)) = name.strip_prefix("http:").and_then(|r| r.split_once(' ')) else {
            continue;
        };
        if served.contains(&name) || path.matches('/').count() < 2 {
            continue;
        }
        let cands: Vec<&&String> = served
            .iter()
            .filter(|p| {
                p.strip_prefix("http:")
                    .and_then(|r| r.split_once(' '))
                    .is_some_and(|(v, pp)| v == verb && pp != path && pp.ends_with(path))
            })
            .collect();
        let mut uniq: Vec<&String> = cands.into_iter().copied().collect();
        uniq.sort();
        uniq.dedup();
        if let [target] = uniq.as_slice() {
            redirect.insert(id.clone(), contract_id(target).render());
        }
    }
    for e in &mut out.edges {
        if let Some(t) = redirect.get(&e.1) {
            e.1 = t.clone();
        }
    }
    out.symbols
        .retain(|s| !redirect.contains_key(&s.id.render()));
}

struct Cx<'b, 'a> {
    b: &'b Binder<'a>,
    values: HashMap<(&'a str, &'a str), &'a ValueExpr>,
    /// Client registry: (scope, target name) → client kind (+ base URL value).
    clients: HashMap<(String, String), Client>,
    /// Router variable id → own prefix.
    routers: HashMap<String, String>,
    /// Router variable id → (parent app/router id, include prefix).
    mounts: HashMap<String, Vec<(String, String)>>,
    /// Classes/interfaces whose methods (transitively) perform HTTP, plus their supertypes.
    http_types: std::collections::HashSet<String>,
    /// Go route groups: (scope, variable) → (parent receiver, prefix).
    groups: HashMap<(String, String), (Vec<String>, String)>,
    /// Repo → (dotted config key, value), from YAML config files, in file order.
    config: HashMap<&'a str, Vec<(&'a str, &'a str)>>,
    /// Callable id → parameter names (receiver excluded).
    params: HashMap<&'a str, &'a [String]>,
    /// Callable id → what it does with its parameters (`send_message(QueueUrl=<param>)`).
    summaries: std::cell::RefCell<HashMap<String, Vec<Summary>>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Scheme {
    Sqs,
    Sns,
    S3,
}

/// A callable passes its parameter `param` (position `index`) to a contract operation.
#[derive(Clone, PartialEq)]
struct Summary {
    param: String,
    index: usize,
    /// Attribute path read off the parameter (`config.queue` → `["queue"]`).
    field: Vec<String>,
    scheme: Scheme,
    kind: EdgeKind,
}

/// A parameter origin: callable, parameter name, position, attribute path.
type Origin = (String, String, usize, Vec<String>);

/// A parameter's position, or `usize::MAX` when it is keyword-only (after a `*` marker).
fn positional(names: &[String], p: &str) -> Option<usize> {
    let i = names.iter().position(|n| n == p)?;
    Some(if names[..i].iter().any(|n| n == "*") {
        usize::MAX
    } else {
        i
    })
}

fn scheme_key(s: Scheme, v: &str) -> String {
    match s {
        Scheme::Sqs => format!("sqs:{}", untier(&queue_name(v))),
        Scheme::Sns => format!("sns:{}", untier(&arn_name(v))),
        Scheme::S3 => format!("s3:{}", untier(&bucket(v))),
    }
}

/// boto3 operations: the argument naming the resource (keyword, position) and the edge.
fn aws_spec(svc: &str, method: &str) -> Option<(&'static [&'static str], usize, Scheme, EdgeKind)> {
    const ANY: usize = usize::MAX;
    Some(match (svc, method) {
        ("sqs", "send_message" | "send_message_batch") => {
            (&["QueueUrl"], ANY, Scheme::Sqs, EdgeKind::Publishes)
        }
        ("sqs", "receive_message") => (&["QueueUrl"], ANY, Scheme::Sqs, EdgeKind::Subscribes),
        ("sns", "publish" | "publish_batch") => (
            &["TopicArn", "TargetArn"],
            ANY,
            Scheme::Sns,
            EdgeKind::Publishes,
        ),
        ("s3", "put_object" | "delete_object" | "copy_object") => {
            (&["Bucket"], ANY, Scheme::S3, EdgeKind::Writes)
        }
        ("s3", "upload_file" | "upload_fileobj") => (&["Bucket"], 1, Scheme::S3, EdgeKind::Writes),
        ("s3", "get_object" | "head_object" | "list_objects_v2") => {
            (&["Bucket"], ANY, Scheme::S3, EdgeKind::Reads)
        }
        ("s3", "download_file" | "download_fileobj") => {
            (&["Bucket"], 0, Scheme::S3, EdgeKind::Reads)
        }
        _ => return None,
    })
}

/// The AWS service a call targets, from boto3's distinctive keyword names, when the client
/// itself was built somewhere we cannot see (`client = await self._sqs()`).
fn infer_aws(method: &str, has: impl Fn(&str) -> bool) -> Option<&'static str> {
    ["sqs", "sns", "s3"]
        .into_iter()
        .find(|svc| aws_spec(svc, method).is_some_and(|(kws, _, _, _)| kws.iter().any(|k| has(k))))
}

/// Attribute names that denote the resource itself (`topic.arn`, `queue.url`).
const RESOURCE_ATTRS: &[&str] = &["arn", "url", "name", "id"];

impl<'b, 'a> Cx<'b, 'a> {
    fn new(b: &'b Binder<'a>, facts: &'a [FileFacts]) -> Self {
        let mut cx = Cx {
            b,
            values: HashMap::new(),
            clients: HashMap::new(),
            routers: HashMap::new(),
            mounts: HashMap::new(),
            http_types: std::collections::HashSet::new(),
            groups: HashMap::new(),
            params: HashMap::new(),
            summaries: Default::default(),
            config: HashMap::new(),
        };
        for f in facts
            .iter()
            .filter(|f| f.file_id.starts_with(crate::configvals::SCOPE))
        {
            for v in &f.values {
                if let ValueExpr::Lit(lit) = &v.value {
                    cx.config
                        .entry(f.repo.as_str())
                        .or_default()
                        .push((v.name.as_str(), lit.as_str()));
                }
            }
        }
        for f in facts {
            for p in &f.params {
                cx.params.insert(p.func.as_str(), p.names.as_slice());
            }
        }
        for f in facts {
            for v in &f.values {
                cx.values
                    .insert((v.scope.as_str(), v.name.as_str()), &v.value);
            }
        }
        for f in facts {
            for c in &f.contracts {
                let ContractFact::Construct {
                    scope,
                    target,
                    ctor,
                    args,
                } = c
                else {
                    continue;
                };
                if f.language == Language::Go && ctor.last().is_some_and(|c| c == "Group") {
                    let prefix = args
                        .positional
                        .first()
                        .and_then(Option::as_ref)
                        .and_then(|v| cx.value(v, scope, 0))
                        .unwrap_or_default();
                    let parent = ctor[..ctor.len() - 1].to_vec();
                    if parent == [target.clone()] {
                        continue;
                    }
                    cx.groups
                        .insert((scope.clone(), target.clone()), (parent, prefix));
                    continue;
                }
                let Some(ext) = cx.external(ctor, scope, f) else {
                    continue;
                };
                if ROUTERS.contains(&ext.as_str()) {
                    if let Some(var) = cx.var_of(std::slice::from_ref(target), scope, f) {
                        let prefix = args
                            .get("prefix", usize::MAX)
                            .and_then(|v| cx.value(v, scope, 0))
                            .unwrap_or_default();
                        cx.routers.insert(var, prefix);
                    }
                } else if HTTP_CLIENTS.contains(&ext.as_str()) {
                    cx.clients
                        .insert((scope.clone(), target.clone()), Client::Http);
                } else if ext == "boto3.client" || ext == "boto3.resource" {
                    if let Some(svc) = args
                        .get("service_name", 0)
                        .and_then(|v| cx.value(v, scope, 0))
                    {
                        cx.clients
                            .insert((scope.clone(), target.clone()), Client::Aws(svc));
                    }
                }
            }
        }
        for f in facts {
            for c in &f.contracts {
                let ContractFact::Op {
                    from,
                    receiver,
                    method,
                    args,
                    ..
                } = c
                else {
                    continue;
                };
                // FastAPI `include_router(r, prefix=…)`, Express `use("/prefix", r)`.
                let (router, prefix) = match method.as_str() {
                    "include_router" => (
                        args.positional.first(),
                        args.get("prefix", usize::MAX)
                            .and_then(|v| cx.value(v, from, 0)),
                    ),
                    "use" if args.positional.len() >= 2 => (
                        args.positional.last(),
                        args.positional
                            .first()
                            .and_then(Option::as_ref)
                            .and_then(|v| cx.value(v, from, 0)),
                    ),
                    _ => continue,
                };
                let parent = cx.var_of(receiver, from, f);
                let router = match router {
                    Some(Some(ValueExpr::Name(p))) => cx.var_of(p, from, f),
                    _ => None,
                };
                if let (Some(parent), Some(router)) = (parent, router) {
                    cx.mounts
                        .entry(router)
                        .or_default()
                        .push((parent, prefix.unwrap_or_default()));
                }
            }
        }
        cx
    }

    /// Mark types whose methods reach `fetch`/`axios` through the call graph (TS/JS wrappers).
    fn performing(&mut self, facts: &[FileFacts], calls: &[(String, String)]) {
        let mut doing: std::collections::HashSet<String> = std::collections::HashSet::new();
        for f in facts
            .iter()
            .filter(|f| matches!(f.language, Language::TypeScript | Language::JavaScript))
        {
            for c in &f.contracts {
                if let ContractFact::Op {
                    from,
                    receiver,
                    method,
                    ..
                } = c
                {
                    if (receiver.is_empty() && method == "fetch")
                        || self.direct_http(receiver, from, f)
                    {
                        doing.insert(from.clone());
                    }
                }
            }
        }
        let mut callers: HashMap<&str, Vec<&str>> = HashMap::new();
        for (src, dst) in calls {
            callers.entry(dst.as_str()).or_default().push(src.as_str());
        }
        let mut queue: Vec<String> = doing.iter().cloned().collect();
        while let Some(n) = queue.pop() {
            for c in callers.get(n.as_str()).into_iter().flatten() {
                if doing.len() < 100_000 && doing.insert(c.to_string()) {
                    queue.push(c.to_string());
                }
            }
        }
        let mut types: Vec<String> = doing
            .iter()
            .filter_map(|m| self.b.parent.get(m))
            .filter(|p| self.b.is_type(p))
            .cloned()
            .collect();
        while let Some(t) = types.pop() {
            if self.http_types.insert(t.clone()) {
                types.extend(self.b.bases.borrow().get(&t).cloned().unwrap_or_default());
            }
        }
    }

    fn direct_http(&self, receiver: &[String], from: &str, f: &FileFacts) -> bool {
        receiver.len() == 1
            && self
                .external(receiver, from, f)
                .is_some_and(|m| HTTP_MODULES.contains(&m.as_str()))
    }

    /// Prefix of a Go route group variable, following nested groups.
    fn group_prefix(&self, scope: &str, receiver: &[String], depth: u32) -> String {
        let [name] = receiver else {
            return String::new();
        };
        match self.groups.get(&(scope.to_string(), name.clone())) {
            Some((parent, p)) if depth < 8 => {
                format!("{}{p}", self.group_prefix(scope, parent, depth + 1))
            }
            _ => String::new(),
        }
    }

    /// Full mount prefix of a router: parents' include prefixes, then its own prefix.
    /// Every full mount prefix of a router (a router can be mounted in several places).
    fn prefixes(&self, router: &str, depth: u32) -> Vec<String> {
        let own = self.routers.get(router).cloned().unwrap_or_default();
        match self.mounts.get(router) {
            Some(ms) if depth < 8 => {
                let mut out: Vec<String> = ms
                    .iter()
                    .flat_map(|(parent, p)| {
                        self.prefixes(parent, depth + 1)
                            .into_iter()
                            .map(move |pp| format!("{pp}{p}"))
                    })
                    .map(|pre| format!("{pre}{own}"))
                    .collect();
                out.sort();
                out.dedup();
                out
            }
            _ => vec![own],
        }
    }

    /// Dotted external name of a path whose head is imported from a third-party module.
    fn external(&self, path: &[String], scope: &str, f: &FileFacts) -> Option<String> {
        let (head, rest) = path.split_first()?;
        let base = match self.b.resolve_name(head, scope, f, 0) {
            Some(Value::Module { name, .. })
                if !self
                    .b
                    .modules
                    .contains_key(&(super::Family::Py, name.as_str())) =>
            {
                name
            }
            Some(_) => return None,
            // `from fastapi import APIRouter`: bound by an import the corpus can't resolve.
            None => f.imports.iter().find_map(|imp| {
                imp.items.iter().find_map(|item| match item {
                    // A default import names the module itself (`import axios from "axios"`).
                    ImportItem::Name { name, alias }
                        if imp.level == 0
                            && name == "default"
                            && alias.as_deref() == Some(head) =>
                    {
                        Some(imp.module.clone())
                    }
                    ImportItem::Name { name, alias }
                        if imp.level == 0 && alias.as_deref().unwrap_or(name) == head =>
                    {
                        Some(format!("{}.{name}", imp.module))
                    }
                    ImportItem::Module { alias: Some(a) } if a == head => Some(imp.module.clone()),
                    _ => None,
                })
            })?,
        };
        let mut parts = vec![base];
        parts.extend(rest.iter().cloned());
        Some(parts.join("."))
    }

    /// The module-variable symbol a path names (routers and apps are module variables).
    fn var_of(&self, path: &[String], scope: &str, f: &FileFacts) -> Option<String> {
        let found = match path {
            [name] => self.b.module_member(&f.file_id, name, true, 0),
            _ => self.b.resolve_path(path, scope, f, 0),
        };
        match found? {
            Value::Sym(id) => Some(id),
            _ => None,
        }
    }

    fn client(&self, receiver: &[String], from: &str, f: &FileFacts) -> Option<Client> {
        if let ([recv, attr], true) = (receiver, receiver.first().is_some_and(|r| r == "self")) {
            let class = self.b.enclosing_class(from)?;
            return self
                .clients
                .get(&(class, format!("{recv}.{attr}")))
                .cloned();
        }
        if let [name] = receiver {
            for s in self.b.scope_chain(from) {
                if let Some(c) = self.clients.get(&(s, name.clone())) {
                    return Some(c.clone());
                }
            }
        }
        // A module-level client imported from elsewhere.
        let id = self.var_of(receiver, from, f)?;
        let var = self.b.sym(&id)?;
        let scope = self.b.parent.get(&id)?;
        self.clients
            .get(&(scope.clone(), var.name.clone()))
            .cloned()
    }

    fn op(
        &self,
        f: &FileFacts,
        from: &str,
        receiver: &[String],
        method: &str,
        args: &Args,
        emit: &mut impl FnMut(&str, String, EdgeKind),
    ) {
        if method == "text" && receiver.is_empty() {
            if let Some(ValueExpr::Lit(sql)) = args.positional.first().and_then(Option::as_ref) {
                for (table, write) in sql_tables(sql) {
                    let kind = if write {
                        EdgeKind::Writes
                    } else {
                        EdgeKind::Reads
                    };
                    emit(from, format!("table:{table}"), kind);
                }
            }
            return;
        }
        // pypika: `Query.from_(t)` / `.join(t)` read, `Query.into(t)` / `Query.update(t)` write.
        if matches!(method, "from_" | "join" | "into" | "update") {
            if let Some(table) = args
                .positional
                .first()
                .and_then(Option::as_ref)
                .and_then(|v| self.pypika_table(v, from))
            {
                let kind = if matches!(method, "into" | "update") {
                    EdgeKind::Writes
                } else {
                    EdgeKind::Reads
                };
                emit(from, format!("table:{table}"), kind);
                return;
            }
        }
        if let Some(table) = self.model_table(args, from, f) {
            if DB_READS.contains(&method) {
                emit(from, format!("table:{table}"), EdgeKind::Reads);
                return;
            }
            if DB_WRITES.contains(&method) {
                emit(from, format!("table:{table}"), EdgeKind::Writes);
                return;
            }
        }
        // Express: `router.get("/x", …, handler)` on an `express()` / `Router()` instance.
        if HTTP_VERBS.contains(&method)
            && matches!(f.language, Language::TypeScript | Language::JavaScript)
            && args.positional.len() >= 2
        {
            if let Some(router) = self
                .var_of(receiver, from, f)
                .filter(|r| self.routers.contains_key(r))
            {
                let path = args.positional.first().and_then(Option::as_ref);
                if let Some(p) = path.and_then(|p| self.value(p, from, 0)) {
                    let handler = match args.positional.last() {
                        Some(Some(ValueExpr::Name(h))) => {
                            match self.b.resolve_path(h, from, f, 0) {
                                Some(Value::Sym(id)) => id,
                                _ => from.to_string(),
                            }
                        }
                        _ => from.to_string(),
                    };
                    for pre in self.prefixes(&router, 0) {
                        emit(
                            &handler,
                            format!(
                                "http:{} {}",
                                method.to_uppercase(),
                                http_path(&format!("{pre}{p}"))
                            ),
                            EdgeKind::Serves,
                        );
                    }
                }
                return;
            }
        }
        if receiver.is_empty() && method == "fetch" {
            let url = args.positional.first().and_then(Option::as_ref);
            let verb = match args.positional.get(1).and_then(Option::as_ref) {
                Some(init) => self
                    .field(init, from, "method", 0)
                    .and_then(|(v, at)| self.value(&v, &at, 0))
                    .unwrap_or_else(|| "GET".into()),
                None => "GET".into(),
            };
            if let Some(url) = url.and_then(|u| self.value(u, from, 0)) {
                emit(
                    from,
                    format!("http:{} {}", verb.to_uppercase(), http_path(&url)),
                    EdgeKind::Consumes,
                );
            }
            return;
        }
        if matches!(f.language, Language::Java | Language::Kotlin)
            && receiver == [crate::lang::spring::WEBCLIENT]
        {
            let url = args.positional.first().and_then(Option::as_ref);
            if let Some(url) = url.and_then(|u| self.value(u, from, 0)) {
                emit(
                    from,
                    format!("http:{method} {}", http_path(&url)),
                    EdgeKind::Consumes,
                );
            }
            return;
        }
        if matches!(f.language, Language::Java | Language::Kotlin) {
            if let Some((_, verb)) = crate::lang::spring::REST_TEMPLATE
                .iter()
                .find(|(m, _)| *m == method)
            {
                // `exchange(url, HttpMethod.GET, …)` names its verb.
                let verb = match (verb.is_empty(), args.positional.get(1)) {
                    (false, _) => Some(verb.to_string()),
                    (true, Some(Some(ValueExpr::Name(p)))) => p.last().cloned(),
                    _ => None,
                };
                let url = args.positional.first().and_then(Option::as_ref);
                if let (Some(verb), Some(url)) = (verb, url.and_then(|u| self.value(u, from, 0))) {
                    emit(
                        from,
                        format!("http:{} {}", verb.to_uppercase(), http_path(&url)),
                        EdgeKind::Consumes,
                    );
                }
                return;
            }
        }
        if f.language == Language::Go {
            if let Some((verb, url)) = go_request(receiver, method, args) {
                if let Some(url) = self.value(url, from, 0) {
                    emit(
                        from,
                        format!("http:{verb} {}", http_path(&url)),
                        EdgeKind::Consumes,
                    );
                }
                return;
            }
        }
        if f.language == Language::Rust && HTTP_VERBS.contains(&method) {
            // Only URL-shaped arguments: `map.get(key)` is not a request.
            if let Some(url) = args
                .positional
                .first()
                .and_then(Option::as_ref)
                .and_then(|u| self.value(u, from, 0))
                .filter(|u| u.starts_with("http") || u.starts_with('/'))
            {
                emit(
                    from,
                    format!("http:{} {}", method.to_uppercase(), http_path(&url)),
                    EdgeKind::Consumes,
                );
            }
            return;
        }
        let direct_http = self.direct_http(receiver, from, f);
        let client = if direct_http {
            Some(Client::Http)
        } else {
            self.client(receiver, from, f)
                .or_else(|| self.wrapper(receiver, method, from, f))
        };
        match client {
            Some(Client::Http) if HTTP_VERBS.contains(&method) || method == "request" => {
                let (verb, url) = if method == "request" {
                    (
                        args.get("method", 0).and_then(|v| self.value(v, from, 0)),
                        args.get("url", 1),
                    )
                } else {
                    (Some(method.to_string()), args.get("url", 0))
                };
                if let (Some(verb), Some(url)) = (verb, url.and_then(|u| self.value(u, from, 0))) {
                    emit(
                        from,
                        format!("http:{} {}", verb.to_uppercase(), http_path(&url)),
                        EdgeKind::Consumes,
                    );
                }
            }
            Some(Client::Aws(svc)) => {
                if let Some((key, kind)) = self.aws(&svc, method, args, from) {
                    emit(from, key, kind);
                }
            }
            None => {
                if let Some(svc) = infer_aws(method, |k| self.kwarg(args, k, from).is_some()) {
                    if let Some((key, kind)) = self.aws(svc, method, args, from) {
                        emit(from, key, kind);
                    }
                }
            }
            _ => {}
        }
    }

    /// A verb method on a class that owns an HTTP client (directly or via its bases) is an
    /// HTTP call: shared service clients are recognised without per-wrapper rules.
    fn wrapper(
        &self,
        receiver: &[String],
        method: &str,
        from: &str,
        f: &FileFacts,
    ) -> Option<Client> {
        if !(HTTP_VERBS.contains(&method) || method == "request") || receiver.is_empty() {
            return None;
        }
        let mut full = receiver.to_vec();
        full.push(method.to_string());
        if matches!(f.language, Language::TypeScript | Language::JavaScript) {
            // The receiver's static type (often an interface) is implemented by an HTTP class.
            let Value::Instance(t) = self.b.resolve_path(receiver, from, f, 0)? else {
                return None;
            };
            return self.http_types.contains(&t).then_some(Client::Http);
        }
        let Value::Sym(m) = self.b.resolve_path(&full, from, f, 0)? else {
            return None;
        };
        let owner = self.b.parent.get(&m)?;
        self.owns_http(owner, 0).then_some(Client::Http)
    }

    fn owns_http(&self, class: &str, depth: u32) -> bool {
        if depth > 8 {
            return false;
        }
        let own = self.clients.iter().any(|((scope, target), c)| {
            scope == class && target.starts_with("self.") && matches!(c, Client::Http)
        });
        own || self
            .b
            .bases
            .borrow()
            .get(class)
            .is_some_and(|bs| bs.iter().any(|b| self.owns_http(b, depth + 1)))
    }

    /// The contract an AWS call names; a resource passed in as a parameter becomes a summary
    /// of the enclosing callable instead, resolved later at its call sites.
    fn aws(&self, svc: &str, method: &str, args: &Args, from: &str) -> Option<(String, EdgeKind)> {
        let (kws, pos, scheme, kind) = aws_spec(svc, method)?;
        let (expr, at) = kws
            .iter()
            .find_map(|k| self.kwarg(args, k, from))
            .or_else(|| {
                args.positional
                    .get(pos)
                    .and_then(Option::as_ref)
                    .map(|e| (e.clone(), from.to_string()))
            })?;
        if let Some(v) = self.value(&expr, &at, 0) {
            return Some((scheme_key(scheme, &v), kind));
        }
        if let Some((func, param, index, field)) = self.param_origin(&expr, &at, 0) {
            self.summarise(
                func,
                Summary {
                    param,
                    index,
                    field,
                    scheme,
                    kind,
                },
            );
        }
        None
    }

    /// A keyword argument, also when passed inside `**params` (`params = dict(TopicArn=…)`);
    /// returns the expression and the scope to read it in.
    fn kwarg(&self, args: &Args, key: &str, from: &str) -> Option<(ValueExpr, String)> {
        if let Some((_, v)) = args.keywords.iter().find(|(n, _)| n == key) {
            return Some((v.clone(), from.to_string()));
        }
        args.keywords
            .iter()
            .filter(|(n, _)| n == "**")
            .find_map(|(_, v)| {
                let (expr, at) = match v {
                    ValueExpr::Name(p) => self.locate(p, from, 0)?,
                    other => (other.clone(), from.to_string()),
                };
                let found = match &expr {
                    ValueExpr::Call { kwargs, .. } => kwargs.iter().find(|(k, _)| k == key),
                    ValueExpr::Record(pairs) => pairs.iter().find(|(k, _)| k == key),
                    _ => None,
                };
                found.map(|(_, v)| (v.clone(), at))
            })
    }

    fn summarise(&self, func: String, s: Summary) -> bool {
        let mut all = self.summaries.borrow_mut();
        let list = all.entry(func).or_default();
        if list.contains(&s) {
            return false;
        }
        list.push(s);
        true
    }

    /// The callable parameter an expression stands for: a parameter (or an attribute of one) of
    /// an enclosing callable, or `self.attr` assigned from a constructor parameter.
    fn param_origin(&self, expr: &ValueExpr, from: &str, depth: u32) -> Option<Origin> {
        let ValueExpr::Name(path) = expr else {
            return None;
        };
        if let [recv, attr, rest @ ..] = path.as_slice() {
            if recv == "self" || recv == "this" {
                let class = self.b.enclosing_class(from)?;
                let (f, p, i, mut field) = self.attr_origin(&class, attr, depth)?;
                field.extend(rest.iter().cloned());
                return Some((f, p, i, field));
            }
        }
        let (head, rest) = path.split_first()?;
        let direct = self.b.scope_chain(from).into_iter().find_map(|s| {
            let i = positional(self.params.get(s.as_str())?, head)?;
            Some((s, head.clone(), i, rest.to_vec()))
        });
        if direct.is_some() || depth > 6 {
            return direct;
        }
        // A local assigned from a parameter (`arn = topic.arn`): follow it.
        let ValueExpr::Name(inner) = self.values.get(&(from, head.as_str())).copied()? else {
            return None;
        };
        let mut next = inner.clone();
        next.extend(rest.iter().cloned());
        self.param_origin(&ValueExpr::Name(next), from, depth + 1)
    }

    fn attr_origin(&self, class: &str, attr: &str, depth: u32) -> Option<Origin> {
        if depth > 6 {
            return None;
        }
        if let Some(ValueExpr::Name(v)) = self.values.get(&(class, attr)).copied() {
            if let Some((q, rest)) = v.split_first() {
                let init = self.init_of(class, 0)?;
                let i = positional(self.params.get(init.as_str())?, q)?;
                return Some((init, q.clone(), i, rest.to_vec()));
            }
        }
        let bases = self
            .b
            .bases
            .borrow()
            .get(class)
            .cloned()
            .unwrap_or_default();
        bases
            .iter()
            .find_map(|b| self.attr_origin(b, attr, depth + 1))
    }

    /// The constructor of a class (`__init__`), inherited if the class declares none.
    fn init_of(&self, class: &str, depth: u32) -> Option<String> {
        let own = format!("{}/__init__().", class.strip_suffix('#')?);
        if self.params.contains_key(own.as_str()) {
            return Some(own);
        }
        if depth > 6 {
            return None;
        }
        let bases = self
            .b
            .bases
            .borrow()
            .get(class)
            .cloned()
            .unwrap_or_default();
        bases.iter().find_map(|b| self.init_of(b, depth + 1))
    }

    /// Resolve summaries at call sites: a call passing a known value emits the contract from the
    /// caller; one passing its own parameter summarises the caller in turn (bounded rounds).
    fn propagate(&self, facts: &[FileFacts], emit: &mut impl FnMut(&str, String, EdgeKind)) {
        for _ in 0..4 {
            let known = self.summaries.borrow().clone();
            if known.is_empty() {
                return;
            }
            let mut grew = false;
            for f in facts {
                for c in &f.calls {
                    let target = match self.b.resolve_path(&c.callee, &c.from, f, 0) {
                        Some(Value::Sym(id)) if self.b.is_type(&id) => self.init_of(&id, 0),
                        Some(Value::Sym(id)) => Some(id),
                        _ => None,
                    };
                    let Some(sums) = target.and_then(|t| known.get(&t)) else {
                        continue;
                    };
                    for s in sums {
                        let Some(arg) = c.args.get(&s.param, s.index) else {
                            continue;
                        };
                        let expr = match (arg, s.field.is_empty()) {
                            (_, true) => arg.clone(),
                            (ValueExpr::Name(p), false) => {
                                ValueExpr::Name(p.iter().chain(&s.field).cloned().collect())
                            }
                            _ => continue,
                        };
                        if let Some(v) = self.value(&expr, &c.from, 0) {
                            emit(&c.from, scheme_key(s.scheme, &v), s.kind.clone());
                        } else if let Some((func, param, index, field)) =
                            self.param_origin(&expr, &c.from, 0)
                        {
                            let next = Summary {
                                param,
                                index,
                                field,
                                ..s.clone()
                            };
                            grew |= self.summarise(func, next);
                        }
                    }
                }
            }
            if !grew {
                return;
            }
        }
    }

    /// The table name of a pypika `Table("name", schema=…)` an expression evaluates to.
    fn pypika_table(&self, v: &ValueExpr, scope: &str) -> Option<String> {
        let (expr, at) = match v {
            ValueExpr::Name(p) => self.locate(p, scope, 0)?,
            other => (other.clone(), scope.to_string()),
        };
        let ValueExpr::Call { func, args, kwargs } = &expr else {
            return None;
        };
        if func.last().map(String::as_str) != Some("Table") {
            return None;
        }
        let name = kwargs
            .iter()
            .find(|(k, _)| k == "name")
            .map(|(_, v)| v)
            .or(args.first())?;
        self.value(name, &at, 0)
    }

    /// Table of the ORM model an operation's first argument names or constructs.
    fn model_table(&self, args: &Args, from: &str, f: &FileFacts) -> Option<String> {
        let path = match args.positional.first() {
            Some(Some(ValueExpr::Name(p))) => p.clone(),
            _ => args.calls.iter().find(|(i, _)| *i == 0)?.1.clone(),
        };
        let Value::Sym(class) = self.b.resolve_path(&path, from, f, 0)? else {
            return None;
        };
        self.b.facts.values().find_map(|ff| {
            ff.contracts.iter().find_map(|c| match c {
                ContractFact::Table { class: c, name } if *c == class => Some(name.clone()),
                _ => None,
            })
        })
    }

    /// The string a value expression evaluates to; unknown parts become `{}`.
    fn value(&self, v: &ValueExpr, scope: &str, depth: u32) -> Option<String> {
        if depth > 12 {
            return None;
        }
        match v {
            ValueExpr::Lit(s) => Some(s.clone()),
            ValueExpr::Env(k) => Some(format!("{{env:{k}}}")),
            ValueExpr::Template(parts) => Some(
                parts
                    .iter()
                    .map(|p| match p {
                        TemplatePart::Lit(s) => s.clone(),
                        TemplatePart::Expr(Some(path)) => self
                            .value(&ValueExpr::Name(path.clone()), scope, depth + 1)
                            .filter(|s| !s.contains("{env:"))
                            .unwrap_or_else(|| "{}".into()),
                        TemplatePart::Expr(None) => "{}".into(),
                    })
                    .collect(),
            ),
            ValueExpr::Name(path) => match self.locate(path, scope, depth + 1) {
                Some((expr, at)) => self.value(&expr, &at, depth + 1),
                None => self.config_value(path, scope).or_else(|| {
                    // `topic.arn`, `queue.url`: the resource the object names.
                    let (last, rest) = path.split_last()?;
                    (!rest.is_empty() && RESOURCE_ATTRS.contains(&last.as_str()))
                        .then(|| self.value(&ValueExpr::Name(rest.to_vec()), scope, depth + 1))
                        .flatten()
                }),
            },
            ValueExpr::Join { sep, parts } => {
                let vals: Vec<String> = parts
                    .iter()
                    .map(|p| {
                        self.value(p, scope, depth + 1)
                            .unwrap_or_else(|| "{}".into())
                    })
                    .collect();
                Some(if sep == "/" {
                    join_path(&vals)
                } else {
                    vals.concat()
                })
            }
            ValueExpr::Call { func, args, .. } => {
                let last = func.last()?.as_str();
                if PASS_THROUGH.contains(&last) {
                    self.value(args.first()?, scope, depth + 1)
                } else if *func == ["fmt", "Sprintf"] || *func == ["String", "format"] {
                    let fmt = self.value(args.first()?, scope, depth + 1)?;
                    Some(printf_placeholders(&fmt))
                } else {
                    None
                }
            }
            ValueExpr::Record(_) => None,
        }
    }

    /// A settings path the code cannot resolve, read from the repo's YAML config: the path below
    /// a settings root (`config.task_queue.queue`), or a unique match of its last 2+ segments.
    fn config_value(&self, path: &[String], scope: &str) -> Option<String> {
        let repo = self.b.file_facts_of(scope)?.repo.as_str();
        let entries = self.config.get(repo)?;
        let segs: Vec<&str> = path.iter().map(String::as_str).collect();
        let root = segs.first()?.to_ascii_lowercase();
        if segs.len() >= 2
            && (root.contains("settings") || root.contains("config") || root == "cfg")
        {
            let key = segs[1..].join(".");
            if let Some((_, v)) = entries.iter().find(|(k, _)| *k == key) {
                return Some(v.to_string());
            }
        }
        for k in (2..=segs.len()).rev() {
            let suffix = segs[segs.len() - k..].join(".");
            let dotted = format!(".{suffix}");
            let hits: std::collections::BTreeSet<&str> = entries
                .iter()
                .filter(|(key, _)| *key == suffix || key.ends_with(&dotted))
                .map(|(_, v)| *v)
                .collect();
            match hits.len() {
                0 => continue,
                1 => return hits.into_iter().next().map(str::to_string),
                _ => return None,
            }
        }
        None
    }

    /// The expression a dotted name denotes and the scope to read it in, following module
    /// constants, class attributes (incl. bases), instance types and record fields.
    fn locate(&self, path: &[String], scope: &str, depth: u32) -> Option<(ValueExpr, String)> {
        if depth > 12 {
            return None;
        }
        let f = self.b.file_facts_of(scope)?;
        let head = path.first()?;
        let mut found = None;
        for s in self.b.scope_chain(scope) {
            if let Some(v) = self.values.get(&(s.as_str(), head.as_str())) {
                found = Some(((*v).clone(), s, 1));
                break;
            }
        }
        if found.is_none() {
            for k in (1..=path.len()).rev() {
                let Some(v) = self.b.resolve_path(&path[..k], scope, f, 0) else {
                    continue;
                };
                let owner = match v {
                    Value::Instance(c) => Some(c),
                    Value::Sym(id) if self.b.is_type(&id) => Some(id),
                    Value::Sym(id) => {
                        // A module variable: the instance it holds (when an attribute follows),
                        // else its own value.
                        let var = self.b.sym(&id)?;
                        let parent = self.b.parent.get(&id)?.clone();
                        let instance = self
                            .b
                            .types
                            .get(&(parent.as_str(), var.name.as_str()))
                            .and_then(|src| self.b.type_of(src, &parent, 0))
                            .and_then(|v| match v {
                                Value::Instance(c) => Some(c),
                                _ => None,
                            });
                        if let (Some(c), Some(attr)) = (&instance, path.get(k)) {
                            if let Some((v, at)) = self.class_value(c, attr, 0) {
                                found = Some((v, at, k + 1));
                                break;
                            }
                        }
                        if let Some(v) = self.values.get(&(parent.as_str(), var.name.as_str())) {
                            found = Some(((*v).clone(), parent, k));
                            break;
                        }
                        instance
                    }
                    _ => None,
                };
                let (Some(class), Some(attr)) = (owner, path.get(k)) else {
                    continue;
                };
                if let Some((v, at)) = self.class_value(&class, attr, 0) {
                    found = Some((v, at, k + 1));
                    break;
                }
            }
        }
        let (mut expr, mut at, used) = found?;
        for name in &path[used..] {
            let (e, a) = self.field(&expr, &at, name, depth + 1)?;
            expr = e;
            at = a;
        }
        Some((expr, at))
    }

    /// `X.name` where `X` evaluates to a record-like value.
    fn field(
        &self,
        v: &ValueExpr,
        scope: &str,
        name: &str,
        depth: u32,
    ) -> Option<(ValueExpr, String)> {
        match v {
            ValueExpr::Record(fields) => fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| (v.clone(), scope.to_string())),
            ValueExpr::Call { func, kwargs, .. }
                if func.last().is_some_and(|f| RECORDS.contains(&f.as_str())) =>
            {
                kwargs
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| (v.clone(), scope.to_string()))
            }
            ValueExpr::Name(p) => {
                let mut full = p.clone();
                full.push(name.to_string());
                self.locate(&full, scope, depth + 1)
            }
            _ => None,
        }
    }

    /// A class attribute's value expression, searching base classes.
    fn class_value(&self, class: &str, attr: &str, depth: u32) -> Option<(ValueExpr, String)> {
        if depth > 8 {
            return None;
        }
        if let Some(v) = self.values.get(&(class, attr)) {
            return Some(((*v).clone(), class.to_string()));
        }
        let bases = self
            .b
            .bases
            .borrow()
            .get(class)
            .cloned()
            .unwrap_or_default();
        bases
            .iter()
            .find_map(|b| self.class_value(b, attr, depth + 1))
    }
}

/// Join path parts with single slashes, keeping a leading slash.
/// `http.Get(url)`, `http.Post(url, …)`, `http.NewRequest("GET", url, …)` and
/// `http.NewRequestWithContext(ctx, "GET", url, …)`: the verb and the URL argument.
fn go_request<'a>(
    receiver: &[String],
    method: &str,
    args: &'a Args,
) -> Option<(String, &'a ValueExpr)> {
    if receiver != ["http"] {
        return None;
    }
    let at = |i: usize| args.positional.get(i).and_then(Option::as_ref);
    let lit = |i: usize| match at(i) {
        Some(ValueExpr::Lit(v)) => Some(v.to_uppercase()),
        Some(ValueExpr::Name(p)) => p
            .last()
            .and_then(|m| m.strip_prefix("Method").map(str::to_uppercase)),
        _ => None,
    };
    match method {
        "Get" | "Head" => Some((method.to_uppercase(), at(0)?)),
        "Post" | "PostForm" => Some(("POST".into(), at(0)?)),
        "NewRequest" => Some((lit(0)?, at(1)?)),
        "NewRequestWithContext" => Some((lit(1)?, at(2)?)),
        _ => None,
    }
}

fn test_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    let (dirs, file) = p.rsplit_once('/').unwrap_or(("", &p));
    let orig = path.rsplit('/').next().unwrap_or(path);
    dirs.split('/')
        .any(|d| matches!(d, "tests" | "test" | "__tests__" | "e2e" | "testing"))
        || file.starts_with("test_")
        || file == "conftest.py"
        || [
            "_test.py", "_test.go", "_test.rs", ".test.ts", ".test.tsx", ".test.js", ".spec.ts",
            ".spec.tsx", ".spec.js",
        ]
        .iter()
        .any(|s| file.ends_with(s))
        // JVM convention: `UserTest.java`, `UserTests.kt` (not `Latest.java`).
        || ["Test.java", "Tests.java", "Test.kt", "Tests.kt"].iter().any(|s| {
            orig.strip_suffix(s)
                .and_then(|stem| stem.chars().last())
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

/// `%s`, `%d`, `%v`, … become `{}` placeholders.
fn printf_placeholders(fmt: &str) -> String {
    let mut out = String::new();
    let mut it = fmt.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match it.peek() {
            Some('%') => {
                it.next();
                out.push('%');
            }
            Some(_) => {
                while it.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
                out.push_str("{}");
            }
            None => out.push('%'),
        }
    }
    out
}

fn join_path(parts: &[String]) -> String {
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        let p = if i == 0 {
            p.trim_end_matches('/')
        } else {
            p.trim_matches('/')
        };
        if p.is_empty() {
            continue;
        }
        if i > 0 {
            out.push('/');
        }
        out.push_str(p);
    }
    out
}

/// `/v1/courses/{course_id}?x=1` → `/v1/courses/{}`; scheme and host dropped.
pub fn http_path(url: &str) -> String {
    let mut p = url;
    for scheme in ["https://", "http://"] {
        if let Some(rest) = p.strip_prefix(scheme) {
            p = rest.find('/').map_or("/", |i| &rest[i..]);
        }
    }
    // A leading unresolved base (`{}/x`, `{env:BASE}/x`) is a host, not a path segment.
    while p.starts_with('{') {
        p = p.find('}').map_or("", |i| &p[i + 1..]);
    }
    let p = p.split(['?', '#']).next().unwrap_or_default();
    let mut segs: Vec<String> = p
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if (s.starts_with('{') && s.ends_with('}')) || s.starts_with(':') {
                "{}".to_string()
            } else {
                s.to_string()
            }
        })
        .collect();
    // `/docs${query}`: a placeholder glued to the last segment is a query string.
    if let Some(last) = segs.last_mut() {
        if last.len() > 2 && last.ends_with("{}") {
            last.truncate(last.len() - 2);
        }
    }
    format!("/{}", segs.join("/"))
}

/// Deployment tiers are not part of a resource's identity: `acme-dev-jobs` and
/// `acme-${var.tier}-jobs` are the same queue, `acme-{}-jobs`.
pub fn untier(name: &str) -> String {
    const TIERS: &[&str] = &[
        "dev",
        "develop",
        "development",
        "stage",
        "staging",
        "stg",
        "prod",
        "production",
        "prd",
        "uat",
        "preprod",
        "sandbox",
    ];
    let mut out = String::new();
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut String| {
        if TIERS.contains(&w.to_ascii_lowercase().as_str()) {
            out.push_str("{}");
        } else {
            out.push_str(w);
        }
        w.clear();
    };
    for c in name.chars() {
        if matches!(c, '-' | '_' | '.' | '/') {
            flush(&mut word, &mut out);
            out.push(c);
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Tokens of a resource name, without trailing kind words (`-queue`, `-sns-topic.fifo`).
fn name_tokens(name: &str) -> Vec<String> {
    let mut t: Vec<String> = name
        .split(['-', '_', '.', ':', '/'])
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    while t.last().is_some_and(|w| {
        matches!(
            w.as_str(),
            "queue" | "topic" | "sns" | "sqs" | "fifo" | "bucket"
        )
    }) {
        t.pop();
    }
    t
}

fn queue_name(url: &str) -> String {
    if url.starts_with("{env:") {
        return url.to_string();
    }
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .to_string()
}

fn arn_name(arn: &str) -> String {
    if arn.starts_with("{env:") {
        return arn.to_string();
    }
    arn.rsplit(':').next().unwrap_or(arn).to_string()
}

fn bucket(b: &str) -> String {
    b.trim_start_matches("s3://")
        .split('/')
        .next()
        .unwrap_or(b)
        .to_string()
}

/// Tables a SQL string reads or writes (`true` = write).
fn sql_tables(sql: &str) -> Vec<(String, bool)> {
    let Ok(stmts) = Parser::parse_sql(&GenericDialect {}, sql) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let name = |n: &ObjectName| {
        n.to_string()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string()
    };
    let from = |twj: &[TableWithJoins], out: &mut Vec<(String, bool)>| {
        for t in twj {
            for f in std::iter::once(&t.relation).chain(t.joins.iter().map(|j| &j.relation)) {
                if let TableFactor::Table { name: n, .. } = f {
                    out.push((name(n), false));
                }
            }
        }
    };
    for s in stmts {
        match s {
            Statement::Query(q) => {
                if let sqlparser::ast::SetExpr::Select(sel) = q.body.as_ref() {
                    from(&sel.from, &mut out);
                }
            }
            Statement::Insert(i) => {
                let t = i.table.to_string();
                out.push((t.rsplit('.').next().unwrap_or(&t).to_string(), true));
            }
            Statement::Update(u) => {
                if let TableFactor::Table { name: n, .. } = &u.table.relation {
                    out.push((name(n), true));
                }
            }
            Statement::Delete(d) => {
                let tables = match &d.from {
                    sqlparser::ast::FromTable::WithFromKeyword(t)
                    | sqlparser::ast::FromTable::WithoutKeyword(t) => t,
                };
                for t in tables {
                    if let TableFactor::Table { name: n, .. } = &t.relation {
                        out.push((name(n), true));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

pub fn contract_id(key: &str) -> GlobalSymbolId {
    GlobalSymbolId {
        scheme: "comindly-contract".into(),
        package_manager: ".".into(),
        package: CONTRACTS.into(),
        version: ".".into(),
        descriptor: key.into(),
    }
}

fn contract_symbol(key: &str) -> Symbol {
    let zero = Position { line: 0, column: 0 };
    Symbol {
        id: contract_id(key),
        name: key.to_string(),
        kind: SymbolKind::Contract,
        language: Language::Other("contract".into()),
        repo: RepoId(CONTRACTS.into()),
        file_path: String::new(),
        range: Range {
            start: zero,
            end: zero,
        },
        signature: None,
        docstring: None,
        fingerprint: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_only_params_have_no_position() {
        let names: Vec<String> = ["a", "*", "b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(positional(&names, "a"), Some(0));
        assert_eq!(positional(&names, "b"), Some(usize::MAX));
    }

    #[test]
    fn tiers_are_not_identity() {
        assert_eq!(
            untier("courses-dev-api-tasks-queue"),
            "courses-{}-api-tasks-queue"
        );
        assert_eq!(
            untier("courses-{}-api-tasks-queue"),
            "courses-{}-api-tasks-queue"
        );
        assert_eq!(untier("acme_prod.events.fifo"), "acme_{}.events.fifo");
        assert_eq!(untier("developers-feed"), "developers-feed");
        assert_eq!(
            name_tokens("courses-{}-x-analysis-events-queue")
                .last()
                .map(String::as_str),
            Some("events")
        );
    }

    #[test]
    fn test_paths() {
        assert!(test_path("app/tests/e2e/test_x.py"));
        assert!(test_path("src/test/java/com/acme/UserTest.java"));
        assert!(test_path("web/src/api.spec.ts"));
        assert!(test_path("pkg/handler_test.go"));
        assert!(!test_path("app/latest_version.py"));
        assert!(!test_path("src/contest/routes.py"));
        assert!(!test_path("src/main/java/com/acme/Latest.java"));
        assert!(!test_path("src/contests.rs"));
        assert!(test_path("src/main/kotlin/UserServiceTest.kt"));
    }

    #[test]
    fn framework_path_params_and_printf() {
        assert_eq!(http_path("/api/things/:id"), "/api/things/{}");
        assert_eq!(http_path("/api/x/{id:[0-9]+}"), "/api/x/{}");
        assert_eq!(printf_placeholders("%s/api/users/%d"), "{}/api/users/{}");
        assert_eq!(printf_placeholders("100%% %5.2f"), "100% {}");
        assert_eq!(
            http_path(&printf_placeholders("%s/api/users/%s")),
            "/api/users/{}"
        );
    }

    #[test]
    fn http_paths_normalize() {
        assert_eq!(http_path("/v1/courses/{course_id}"), "/v1/courses/{}");
        assert_eq!(
            http_path("http://courses-svc/v1/courses/{}?x=1"),
            "/v1/courses/{}"
        );
        assert_eq!(http_path("{}/v1/a//b/"), "/v1/a/b");
        assert_eq!(http_path("{env:BASE}/x"), "/x");
        assert_eq!(http_path("/api/documents{}"), "/api/documents");
    }

    #[test]
    fn sql_tables_by_statement() {
        assert_eq!(
            sql_tables("SELECT count(*) FROM courses"),
            vec![("courses".into(), false)]
        );
        assert_eq!(
            sql_tables("SELECT * FROM a JOIN b ON a.id = b.id"),
            vec![("a".into(), false), ("b".into(), false)]
        );
        assert_eq!(
            sql_tables("INSERT INTO courses (id) VALUES (1)"),
            vec![("courses".into(), true)]
        );
        assert_eq!(
            sql_tables("UPDATE x.courses SET a = 1"),
            vec![("courses".into(), true)]
        );
    }
}
