//! Conventions engine: measured coding conventions per repo, with support counts, central
//! exemplars and counterexamples, mined from facts and the graph. An LLM may phrase them; it
//! never invents them. Conventions shared by several repos form the org baseline.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use futures::TryStreamExt;
use object_store::path::Path as ObjPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use serde::{Deserialize, Serialize};

use crate::facts::{FileFacts, ImportItem, RefKind, ValueExpr};
use crate::model::{Edge, EdgeKind, Symbol, SymbolKind};

/// Repo name used for org-wide conventions.
pub const ORG: &str = "*";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Convention {
    /// Repo the convention holds in, or [`ORG`] for the org baseline.
    pub repo: String,
    /// `naming`, `signatures`, `models`, `errors`, `layout`, `logging`, `config`, `libraries`,
    /// `testing`.
    pub dimension: String,
    /// Stable identity of the measured choice, shared across repos (`naming:functions:snake_case`).
    pub key: String,
    pub rule: String,
    /// The rule without repo-specific counts, names or paths (used for the org baseline).
    pub generic: String,
    pub support: usize,
    pub total: usize,
    /// `path:line` of representative, central instances.
    pub exemplars: Vec<String>,
    /// `path:line` of instances that deviate.
    pub counterexamples: Vec<String>,
    #[serde(default)]
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    #[default]
    Measured,
    /// Measured, with the rule rewritten by a person.
    Edited,
    /// Written by a person.
    Manual,
}

const MANUAL: &str = "manual:";

impl Convention {
    /// Stable handle for edits: a manual entry's id, or a hash of repo and key.
    pub fn id(&self) -> String {
        match self.key.strip_prefix(MANUAL) {
            Some(id) => id.to_string(),
            None => measured_id(&self.repo, &self.key),
        }
    }
}

fn measured_id(repo: &str, key: &str) -> String {
    let h = blake3::hash(format!("{repo}\n{key}").as_bytes()).to_hex();
    format!("m{}", &h[..10])
}

/// A person's change: a new convention, or an edit or hiding (`replaces`) of a measured one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manual {
    pub id: String,
    /// Repo, or [`ORG`] for the whole org.
    pub repo: String,
    pub dimension: String,
    /// The rule; for a hidden convention, the rule that was hidden.
    pub rule: String,
    /// Key of the measured convention this overrides.
    #[serde(default)]
    pub replaces: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    pub actor: String,
    pub updated: String,
}

impl Manual {
    pub fn new(repo: &str, dimension: &str, rule: &str, actor: &str) -> Manual {
        let id = uuid::Uuid::now_v7().simple().to_string();
        Manual {
            id: format!("c{}", &id[id.len() - 10..]),
            repo: repo.into(),
            dimension: dimension.trim().to_lowercase(),
            rule: rule.trim().into(),
            replaces: None,
            hidden: false,
            actor: actor.into(),
            updated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.rule.trim().is_empty() || self.rule.chars().count() > 500 {
            bail!("rule must be 1–500 characters");
        }
        if self.dimension.is_empty() || self.dimension.len() > 40 {
            bail!("dimension must be 1–40 characters");
        }
        if let Some(why) = crate::ledger::suspicious(&self.rule) {
            bail!("rule rejected: {why}");
        }
        Ok(())
    }
}

/// What a change to one convention writes.
#[derive(Debug, PartialEq)]
pub enum Write {
    Put(Manual),
    Delete(String),
}

pub enum Change {
    Edit(String),
    Hide,
    /// Undo an edit or hiding of a measured convention.
    Restore,
}

/// The write for `change` to the convention `id`; `current` is the applied list.
pub fn change(
    current: &[Convention],
    manual: &[Manual],
    id: &str,
    change: Change,
    actor: &str,
) -> Result<Write> {
    let own = manual.iter().find(|m| m.id == id);
    if let Change::Restore = change {
        return match own {
            Some(m) if m.replaces.is_some() => Ok(Write::Delete(m.id.clone())),
            _ => bail!("{id} is not an edited or hidden measured convention"),
        };
    }
    let c = current
        .iter()
        .find(|c| c.id() == id)
        .with_context(|| format!("no convention {id}"))?;
    let base = || Manual {
        id: id.to_string(),
        repo: c.repo.clone(),
        dimension: c.dimension.clone(),
        rule: c.rule.clone(),
        replaces: (c.origin != Origin::Manual).then(|| c.key.clone()),
        hidden: false,
        actor: actor.into(),
        updated: chrono::Utc::now().format("%Y-%m-%d").to_string(),
    };
    Ok(match change {
        Change::Edit(rule) => Write::Put(Manual {
            rule: rule.trim().into(),
            ..base()
        }),
        Change::Hide if c.origin == Origin::Manual => Write::Delete(id.to_string()),
        Change::Hide => Write::Put(Manual {
            hidden: true,
            rule: own.map_or(c.rule.clone(), |m| m.rule.clone()),
            ..base()
        }),
        Change::Restore => unreachable!(),
    })
}

/// Measured conventions with people's changes applied: hidden ones dropped, edits replace the
/// rule, new ones appended.
pub fn apply(mut measured: Vec<Convention>, manual: &[Manual]) -> Vec<Convention> {
    let over: HashMap<(&str, &str), &Manual> = manual
        .iter()
        .filter_map(|m| Some(((m.repo.as_str(), m.replaces.as_deref()?), m)))
        .collect();
    measured.retain_mut(|c| match over.get(&(c.repo.as_str(), c.key.as_str())) {
        Some(m) if m.hidden => false,
        Some(m) => {
            c.rule = m.rule.clone();
            c.generic = m.rule.clone();
            c.origin = Origin::Edited;
            true
        }
        None => true,
    });
    measured.extend(
        manual
            .iter()
            .filter(|m| m.replaces.is_none())
            .map(|m| Convention {
                repo: m.repo.clone(),
                dimension: m.dimension.clone(),
                key: format!("{MANUAL}{}", m.id),
                rule: m.rule.clone(),
                generic: m.rule.clone(),
                support: 0,
                total: 0,
                exemplars: Vec::new(),
                counterexamples: Vec::new(),
                origin: Origin::Manual,
            }),
    );
    measured
}

/// People's changes, one JSON object each under `<index_root>/_conventions/`; indexing never
/// touches them.
pub struct Store {
    store: Arc<dyn ObjectStore>,
    root: ObjPath,
}

impl Store {
    pub fn open(index_root: &str) -> Result<Store> {
        let (store, root) = crate::ledger::sub_store(index_root, "_conventions")?;
        Ok(Store { store, root })
    }

    pub async fn all(&self) -> Result<Vec<Manual>> {
        let metas: Vec<_> = self.store.list(Some(&self.root)).try_collect().await?;
        let mut out = Vec::new();
        for m in metas
            .iter()
            .filter(|m| m.location.as_ref().ends_with(".json"))
        {
            let bytes = self.store.get(&m.location).await?.bytes().await?;
            match serde_json::from_slice::<Manual>(&bytes) {
                Ok(c) => out.push(c),
                Err(e) => eprintln!(
                    "comindly: skipping unreadable convention {}: {e}",
                    m.location
                ),
            }
        }
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    pub async fn write(&self, w: &Write) -> Result<()> {
        match w {
            Write::Put(m) => {
                m.validate()?;
                let key = self.root.clone().join(format!("{}.json", m.id));
                let body = PutPayload::from(serde_json::to_vec_pretty(m)?);
                self.store
                    .put(&key, body)
                    .await
                    .context("write convention")?;
            }
            Write::Delete(id) => {
                let key = self.root.clone().join(format!("{id}.json"));
                self.store.delete(&key).await.context("delete convention")?;
            }
        }
        Ok(())
    }
}

pub fn manual_blocking(index_root: &str) -> Result<Vec<Manual>> {
    crate::rt::block_on(Store::open(index_root)?.all())
}

pub fn write_blocking(index_root: &str, w: &Write) -> Result<()> {
    crate::rt::block_on(Store::open(index_root)?.write(w))
}

/// External libraries commonly wrapped by internal modules.
const WRAPPABLE: &[&str] = &[
    "httpx",
    "requests",
    "aiohttp",
    "boto3",
    "aiobotocore",
    "botocore",
    "logging",
    "structlog",
    "sqlalchemy",
    "psycopg",
    "psycopg2",
    "asyncpg",
    "redis",
    "openai",
    "anthropic",
    "litellm",
    "axios",
];
const TEST_LIBS: &[&str] = &[
    "pytest",
    "unittest",
    "vitest",
    "jest",
    "@testing-library/react",
    "mocha",
    "hypothesis",
];
const MOCK_LIBS: &[&str] = &[
    "unittest.mock",
    "pytest_mock",
    "respx",
    "moto",
    "freezegun",
    "responses",
    "sinon",
    "msw",
    "nock",
    "fakeredis",
    "pytest_httpx",
];

pub fn mine(symbols: &[Symbol], edges: &[Edge], facts: &[FileFacts]) -> Vec<Convention> {
    let cx = Cx::new(symbols, edges, facts);
    let mut out = Vec::new();
    let mut repos: Vec<&str> = facts.iter().map(|f| f.repo.as_str()).collect();
    repos.sort();
    repos.dedup();
    for repo in repos {
        out.extend(cx.naming(repo));
        out.extend(cx.signatures(repo));
        out.extend(cx.bases(repo));
        out.extend(cx.layout(repo));
        out.extend(cx.logging(repo));
        out.extend(cx.config(repo));
        out.extend(cx.libraries(repo));
        out.extend(cx.testing(repo));
    }
    let org = org_baseline(&out);
    out.extend(org);
    out
}

/// Conventions whose key holds in at least two repos.
fn org_baseline(per_repo: &[Convention]) -> Vec<Convention> {
    let mut by_key: BTreeMap<&str, Vec<&Convention>> = BTreeMap::new();
    for c in per_repo {
        by_key.entry(c.key.as_str()).or_default().push(c);
    }
    let repos: HashSet<&str> = per_repo.iter().map(|c| c.repo.as_str()).collect();
    by_key
        .into_iter()
        .filter(|(_, cs)| cs.len() >= 2)
        .map(|(key, cs)| {
            let names: Vec<&str> = cs.iter().map(|c| c.repo.as_str()).collect();
            Convention {
                origin: Origin::Measured,
                repo: ORG.into(),
                dimension: cs[0].dimension.clone(),
                key: key.to_string(),
                rule: format!(
                    "{} — in {} of {} repos ({})",
                    cs[0].generic,
                    cs.len(),
                    repos.len(),
                    names.join(", ")
                ),
                generic: cs[0].generic.clone(),
                support: cs.len(),
                total: repos.len(),
                // `repo:path:line`, since the baseline spans repositories.
                exemplars: cs
                    .iter()
                    .flat_map(|c| c.exemplars.first().map(|e| format!("{}:{e}", c.repo)))
                    .take(3)
                    .collect(),
                counterexamples: Vec::new(),
            }
        })
        .collect()
}

struct Cx<'a> {
    syms: Vec<&'a Symbol>,
    by_id: HashMap<String, &'a Symbol>,
    facts: &'a [FileFacts],
    /// Incoming dependency edges per symbol id (centrality for exemplars).
    dependents: HashMap<String, usize>,
    /// Outgoing edges by kind per source id.
    out_edges: HashMap<(String, EdgeKind), Vec<String>>,
    /// Internal module names (Python dotted, JS specifiers).
    modules: HashSet<&'a str>,
    /// Internal module → external libraries it imports.
    wraps: HashMap<&'a str, HashSet<String>>,
    /// Callables with at least one typed parameter or local.
    typed: HashSet<&'a str>,
    /// Callables with a declared return type.
    returns: HashSet<&'a str>,
}

impl<'a> Cx<'a> {
    fn new(symbols: &'a [Symbol], edges: &'a [Edge], facts: &'a [FileFacts]) -> Self {
        let mut dependents: HashMap<String, usize> = HashMap::new();
        let mut out_edges: HashMap<(String, EdgeKind), Vec<String>> = HashMap::new();
        for e in edges {
            if crate::graph::DEPENDENCY.contains(&e.kind) {
                *dependents.entry(e.dst.render()).or_default() += 1;
            }
            out_edges
                .entry((e.src.render(), e.kind.clone()))
                .or_default()
                .push(e.dst.render());
        }
        let modules: HashSet<&str> = facts
            .iter()
            .flat_map(|f| f.modules.iter().map(String::as_str))
            .collect();
        let mut wraps: HashMap<&str, HashSet<String>> = HashMap::new();
        let (mut typed, mut returns) = (HashSet::new(), HashSet::new());
        for f in facts {
            for t in &f.types {
                if t.name == crate::parse::RETURN {
                    returns.insert(t.scope.as_str());
                } else {
                    typed.insert(t.scope.as_str());
                }
            }
            let ext: HashSet<String> = f
                .imports
                .iter()
                .filter_map(|i| external_root(&i.module, &modules))
                .collect();
            if !ext.is_empty() {
                for m in &f.modules {
                    wraps
                        .entry(m.as_str())
                        .or_default()
                        .extend(ext.iter().cloned());
                }
            }
        }
        Cx {
            syms: symbols
                .iter()
                .filter(|s| s.kind != SymbolKind::Contract)
                .collect(),
            by_id: symbols.iter().map(|s| (s.id.render(), s)).collect(),
            facts,
            dependents,
            out_edges,
            modules,
            wraps,
            typed,
            returns,
        }
    }

    fn repo_syms<'s>(&'s self, repo: &'s str) -> impl Iterator<Item = &'a Symbol> + 's {
        self.syms
            .iter()
            .copied()
            .filter(move |s| s.repo.0 == repo && !is_test(&s.file_path))
    }

    fn repo_facts<'s>(&'s self, repo: &'s str) -> impl Iterator<Item = &'a FileFacts> + 's {
        self.facts.iter().filter(move |f| f.repo == repo)
    }

    /// Locations of the most central symbols first.
    fn central(&self, mut v: Vec<&Symbol>, n: usize) -> Vec<String> {
        v.sort_by_key(|s| {
            std::cmp::Reverse(self.dependents.get(&s.id.render()).copied().unwrap_or(0))
        });
        v.into_iter().take(n).map(loc).collect()
    }

    fn outgoing(&self, id: &str, kind: EdgeKind) -> impl Iterator<Item = &'a Symbol> + '_ {
        self.out_edges
            .get(&(id.to_string(), kind))
            .into_iter()
            .flatten()
            .filter_map(|d| self.by_id.get(d).copied())
    }

    fn naming(&self, repo: &str) -> Vec<Convention> {
        let groups: [(&str, &[SymbolKind]); 3] = [
            ("functions", &[SymbolKind::Function, SymbolKind::Method]),
            (
                "types",
                &[
                    SymbolKind::Class,
                    SymbolKind::Interface,
                    SymbolKind::Struct,
                    SymbolKind::Trait,
                    SymbolKind::Enum,
                ],
            ),
            ("constants", &[SymbolKind::Constant]),
        ];
        let mut out = Vec::new();
        for (label, kinds) in groups {
            let named: Vec<&Symbol> = self
                .repo_syms(repo)
                .filter(|s| kinds.contains(&s.kind))
                .filter(|s| !(s.name.starts_with("__") && s.name.ends_with("__")))
                .collect();
            if named.len() < 10 {
                continue;
            }
            let mut hist: BTreeMap<&str, Vec<&Symbol>> = BTreeMap::new();
            for s in &named {
                hist.entry(casing(&s.name)).or_default().push(s);
            }
            let Some((case, hits)) = hist.iter().max_by_key(|(_, v)| v.len()) else {
                continue;
            };
            if hits.len() * 10 < named.len() * 8 {
                continue;
            }
            let deviants: Vec<&Symbol> = named
                .iter()
                .filter(|s| casing(&s.name) != *case)
                .copied()
                .collect();
            let generic = format!("{} are {case}", capitalize(label));
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "naming".into(),
                key: format!("naming:{label}:{case}"),
                rule: format!("{generic} ({} of {})", hits.len(), named.len()),
                generic,
                support: hits.len(),
                total: named.len(),
                exemplars: self.central(hits.clone(), 3),
                counterexamples: self.central(deviants, 3),
            });
        }
        out
    }

    fn signatures(&self, repo: &str) -> Vec<Convention> {
        let callables: Vec<&Symbol> = self
            .repo_syms(repo)
            .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
            .filter(|s| s.name != "__init__" && s.name != "constructor")
            .collect();
        if callables.len() < 10 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let (annotated, bare): (Vec<&Symbol>, Vec<&Symbol>) = callables
            .iter()
            .partition(|s| self.returns.contains(s.id.render().as_str()));
        let share = annotated.len() * 100 / callables.len();
        if share >= 60 || share <= 20 {
            let (generic, key, ex, dev) = if share >= 60 {
                (
                    "Functions declare return types",
                    "signatures:return-types:annotated",
                    annotated,
                    bare,
                )
            } else {
                (
                    "Return types are mostly left unannotated",
                    "signatures:return-types:bare",
                    bare,
                    annotated,
                )
            };
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "signatures".into(),
                key: key.into(),
                rule: format!("{generic} ({} of {})", ex.len(), callables.len()),
                generic: generic.into(),
                support: ex.len(),
                total: callables.len(),
                exemplars: self.central(ex, 3),
                counterexamples: self.central(dev, 3),
            });
        }
        let with_params: Vec<&Symbol> = callables
            .iter()
            .filter(|s| s.signature.as_deref().is_some_and(has_params))
            .copied()
            .collect();
        let (typed, untyped): (Vec<&Symbol>, Vec<&Symbol>) = with_params
            .iter()
            .partition(|s| self.typed.contains(s.id.render().as_str()));
        if with_params.len() >= 10 && typed.len() * 10 >= with_params.len() * 7 {
            let generic = "Parameters carry type annotations";
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "signatures".into(),
                key: "signatures:params:typed".into(),
                rule: format!(
                    "{generic} ({} of {} functions with parameters)",
                    typed.len(),
                    with_params.len()
                ),
                generic: generic.into(),
                support: typed.len(),
                total: with_params.len(),
                exemplars: self.central(typed, 3),
                counterexamples: self.central(untyped, 3),
            });
        }
        let asyncs: Vec<&Symbol> = callables
            .iter()
            .filter(|s| {
                s.signature
                    .as_deref()
                    .is_some_and(|g| g.trim_start().starts_with("async "))
            })
            .copied()
            .collect();
        if asyncs.len() * 2 >= callables.len() {
            let generic = "Code is predominantly async";
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "signatures".into(),
                key: "signatures:async".into(),
                rule: format!(
                    "{generic} ({} of {} functions)",
                    asyncs.len(),
                    callables.len()
                ),
                generic: generic.into(),
                support: asyncs.len(),
                total: callables.len(),
                exemplars: self.central(asyncs, 3),
                counterexamples: Vec::new(),
            });
        }
        out
    }

    /// The topmost internal error class above `class` (`NotFoundError` → `AcmeError`).
    fn error_root(&self, class: &'a Symbol) -> &'a Symbol {
        let mut cur = class;
        for _ in 0..8 {
            let parent = self
                .outgoing(&cur.id.render(), EdgeKind::Inherits)
                .find(|p| is_error_name(&p.name));
            match parent {
                Some(p) => cur = p,
                None => break,
            }
        }
        cur
    }

    /// Base classes: data models, errors, and other shared bases.
    fn bases(&self, repo: &str) -> Vec<Convention> {
        let mut by_base: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
        let mut errors: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
        for f in self.repo_facts(repo) {
            for r in f
                .refs
                .iter()
                .filter(|r| matches!(r.kind, RefKind::Inherits | RefKind::Implements))
            {
                let Some(class) = self.by_id.get(&r.from).copied() else {
                    continue;
                };
                if is_test(&class.file_path) {
                    continue;
                }
                let internal = self
                    .outgoing(&r.from, EdgeKind::Inherits)
                    .chain(self.outgoing(&r.from, EdgeKind::Implements))
                    .find(|b| Some(&b.name) == r.path.last());
                let base = match internal {
                    Some(b) => label(&b.name, Some(&b.repo.0)),
                    None => match external_name(f, &r.path, &self.modules) {
                        Some(n) => label(&n, None),
                        None => continue,
                    },
                };
                if is_error_name(r.path.last().map_or("", String::as_str)) {
                    let root = match internal {
                        Some(b) => {
                            let root = self.error_root(b);
                            label(&root.name, Some(&root.repo.0))
                        }
                        None => base.clone(),
                    };
                    errors.entry(root).or_default().push(class);
                } else {
                    by_base.entry(base).or_default().push(class);
                }
            }
        }
        let mut out = Vec::new();
        let mut models: Vec<(String, Vec<&Symbol>)> =
            by_base.into_iter().filter(|(_, c)| c.len() >= 3).collect();
        models.sort_by_key(|(_, c)| std::cmp::Reverse(c.len()));
        for (base, classes) in models.into_iter().take(4) {
            let generic = format!("Classes extend {base}");
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "models".into(),
                key: format!("models:base:{base}"),
                rule: format!("{generic} ({} classes)", classes.len()),
                generic,
                support: classes.len(),
                total: classes.len(),
                exemplars: self.central(classes, 3),
                counterexamples: Vec::new(),
            });
        }
        let total: usize = errors.values().map(Vec::len).sum();
        if let Some((root, classes)) = errors.iter().max_by_key(|(_, c)| c.len()) {
            if total >= 3 && classes.len() * 2 >= total {
                let deviants: Vec<&Symbol> = errors
                    .iter()
                    .filter(|(r, _)| *r != root)
                    .flat_map(|(_, c)| c.iter().copied())
                    .collect();
                let generic = format!("Custom errors derive from {root}");
                out.push(Convention {
                    origin: Origin::Measured,
                    repo: repo.into(),
                    dimension: "errors".into(),
                    key: format!("errors:root:{root}"),
                    rule: format!("{generic} ({} of {total})", classes.len()),
                    generic,
                    support: classes.len(),
                    total,
                    exemplars: self.central(classes.clone(), 3),
                    counterexamples: self.central(deviants, 3),
                });
            }
        }
        out
    }

    /// Where each role lives: route handlers, HTTP clients, database queries.
    fn layout(&self, repo: &str) -> Vec<Convention> {
        let role_of = |kinds: &[EdgeKind]| -> Vec<&Symbol> {
            self.repo_syms(repo)
                .filter(|s| {
                    kinds
                        .iter()
                        .any(|k| self.out_edges.contains_key(&(s.id.render(), k.clone())))
                })
                .collect()
        };
        let roles = [
            ("HTTP route handlers", role_of(&[EdgeKind::Serves])),
            ("HTTP client calls", role_of(&[EdgeKind::Consumes])),
            (
                "Database queries",
                role_of(&[EdgeKind::Reads, EdgeKind::Writes]),
            ),
        ];
        let mut out = Vec::new();
        for (label, syms) in roles {
            if syms.len() < 3 {
                continue;
            }
            let mut dirs: BTreeMap<String, Vec<&Symbol>> = BTreeMap::new();
            for s in &syms {
                dirs.entry(dir_of(&s.file_path)).or_default().push(s);
            }
            let Some((dir, hits)) = dirs.iter().max_by_key(|(_, v)| v.len()) else {
                continue;
            };
            if hits.len() * 2 < syms.len() {
                continue;
            }
            let deviants: Vec<&Symbol> = syms
                .iter()
                .filter(|s| &dir_of(&s.file_path) != dir)
                .copied()
                .collect();
            let mut tail: Vec<&str> = dir.rsplit('/').take(2).collect();
            tail.reverse();
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "layout".into(),
                key: format!("layout:{}", label.to_lowercase().replace(' ', "-")),
                rule: format!(
                    "{label} live in `{dir}/` ({} of {})",
                    hits.len(),
                    syms.len()
                ),
                generic: format!(
                    "{label} live in one directory per service (e.g. `…/{}/`)",
                    tail.join("/")
                ),
                support: hits.len(),
                total: syms.len(),
                exemplars: self.central(hits.clone(), 3),
                counterexamples: self.central(deviants, 3),
            });
        }
        out
    }

    /// Which logger factory module code calls.
    fn logging(&self, repo: &str) -> Vec<Convention> {
        let mut by_factory: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for f in self.repo_facts(repo).filter(|f| !is_test(&f.path)) {
            for r in f.refs.iter().filter(|r| r.kind == RefKind::Call) {
                let Some(last) = r.path.last() else { continue };
                if !matches!(
                    last.as_str(),
                    "getLogger" | "get_logger" | "getlogger" | "create_logger"
                ) {
                    continue;
                }
                let name = external_name(f, &r.path, &self.modules)
                    .or_else(|| internal_name(f, &r.path))
                    .unwrap_or_else(|| r.path.join("."));
                let provider = name
                    .rsplit_once('.')
                    .map_or(name.clone(), |(m, _)| m.to_string());
                by_factory
                    .entry(provider)
                    .or_default()
                    .push(format!("{}:{}", f.path, r.line));
            }
        }
        dominant(
            repo,
            "logging",
            "logging:factory",
            by_factory,
            |name| format!("Loggers come from `{name}`"),
            "call sites",
        )
    }

    /// Settings classes vs direct environment reads.
    fn config(&self, repo: &str) -> Vec<Convention> {
        let mut env_reads: Vec<String> = self
            .repo_facts(repo)
            .filter(|f| !is_test(&f.path))
            .flat_map(|f| {
                f.values
                    .iter()
                    .filter(|v| matches!(v.value, ValueExpr::Env(_)))
                    .map(move |_| f.path.clone())
            })
            .collect();
        let settings: Vec<&Symbol> = self
            .repo_syms(repo)
            .filter(|s| s.kind == SymbolKind::Class)
            .filter(|s| {
                let id = s.id.render();
                self.facts.iter().any(|f| {
                    f.refs.iter().any(|r| {
                        r.from == id
                            && r.kind == RefKind::Inherits
                            && r.path
                                .last()
                                .is_some_and(|b| b.ends_with("Settings") || b.ends_with("Config"))
                    })
                })
            })
            .collect();
        if settings.is_empty() && env_reads.len() < 3 {
            return Vec::new();
        }
        let n_env = env_reads.len();
        env_reads.sort();
        env_reads.dedup();
        let (generic, key, rule) = if settings.is_empty() {
            let g = "Configuration is read from environment variables directly";
            (g, "config:env", format!("{g} ({n_env} sites)"))
        } else {
            let g = "Configuration lives in settings classes";
            let names: Vec<String> = settings
                .iter()
                .take(3)
                .map(|s| format!("`{}`", s.name))
                .collect();
            (
                g,
                "config:settings-classes",
                format!("{g} ({}); direct env reads: {n_env}", names.join(", ")),
            )
        };
        vec![Convention {
            origin: Origin::Measured,
            repo: repo.into(),
            dimension: "config".into(),
            key: key.into(),
            rule,
            generic: generic.into(),
            support: settings.len().max(n_env),
            total: settings.len() + n_env,
            exemplars: self.central(settings, 3),
            counterexamples: env_reads.into_iter().take(3).collect(),
        }]
    }

    /// Internal wrappers preferred over direct third-party use (`acme.aws` over `boto3`).
    fn libraries(&self, repo: &str) -> Vec<Convention> {
        let mut direct: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut wrapped: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
        for f in self.repo_facts(repo).filter(|f| !is_test(&f.path)) {
            let own: HashSet<&str> = f.modules.iter().map(String::as_str).collect();
            for imp in &f.imports {
                if let Some(ext) = external_root(&imp.module, &self.modules) {
                    if WRAPPABLE.contains(&ext.as_str()) {
                        direct.entry(ext).or_default().push(f.path.clone());
                    }
                    continue;
                }
                let Some(target) =
                    internal_module(&imp.module, &self.modules).filter(|t| !own.contains(t))
                else {
                    continue;
                };
                for ext in self.wraps.get(target).into_iter().flatten() {
                    if WRAPPABLE.contains(&ext.as_str()) {
                        *wrapped
                            .entry(ext.clone())
                            .or_default()
                            .entry(target.to_string())
                            .or_default() += 1;
                    }
                }
            }
        }
        let mut out = Vec::new();
        for (ext, wrappers) in wrapped {
            let Some((wrapper, n)) = wrappers.iter().max_by_key(|(_, n)| **n) else {
                continue;
            };
            let mut d = direct.get(&ext).cloned().unwrap_or_default();
            d.sort();
            d.dedup();
            // The wrapper's own files don't count as deviations.
            d.retain(|p| {
                !self
                    .modules_of(p)
                    .iter()
                    .any(|m| m.starts_with(wrapper.as_str()))
            });
            if *n < 3 || *n <= d.len() {
                continue;
            }
            let generic = format!("`{ext}` is used through `{wrapper}`");
            out.push(Convention {
                origin: Origin::Measured,
                repo: repo.into(),
                dimension: "libraries".into(),
                key: format!("libraries:{ext}:via:{wrapper}"),
                rule: format!(
                    "{generic} ({n} imports; direct `{ext}` imports: {})",
                    d.len()
                ),
                generic,
                support: *n,
                total: n + d.len(),
                exemplars: Vec::new(),
                counterexamples: d.into_iter().take(3).collect(),
            });
        }
        out
    }

    fn modules_of(&self, path: &str) -> Vec<String> {
        self.facts
            .iter()
            .filter(|f| f.path == path)
            .flat_map(|f| f.modules.clone())
            .collect()
    }

    fn testing(&self, repo: &str) -> Vec<Convention> {
        let tests: Vec<&FileFacts> = self.repo_facts(repo).filter(|f| is_test(&f.path)).collect();
        if tests.len() < 3 {
            return Vec::new();
        }
        let mut fw: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        let mut mocks: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &tests {
            let uses = |lib: &str| {
                f.imports
                    .iter()
                    .any(|i| i.module == lib || i.module.starts_with(&format!("{lib}.")))
            };
            for lib in TEST_LIBS.iter().filter(|l| uses(l)) {
                fw.entry(lib).or_default().push(f.path.clone());
            }
            for lib in MOCK_LIBS.iter().filter(|l| uses(l)) {
                *mocks.entry(lib).or_default() += 1;
            }
        }
        let Some((lib, files)) = fw.iter().max_by_key(|(_, v)| v.len()) else {
            return Vec::new();
        };
        let generic = format!("Tests use `{lib}`");
        let mut rule = format!("{generic} ({} of {} test files)", files.len(), tests.len());
        let mock_list: Vec<String> = mocks.iter().map(|(m, n)| format!("`{m}` ({n})")).collect();
        if !mock_list.is_empty() {
            rule.push_str(&format!("; mocking via {}", mock_list.join(", ")));
        }
        let conftests = tests
            .iter()
            .filter(|f| f.path.ends_with("conftest.py"))
            .count();
        if conftests > 0 {
            rule.push_str(&format!("; shared fixtures in {conftests} conftest.py"));
        }
        let mut ex = files.clone();
        ex.sort_by_key(|p| p.len());
        vec![Convention {
            origin: Origin::Measured,
            repo: repo.into(),
            dimension: "testing".into(),
            key: format!("testing:{lib}"),
            rule,
            generic,
            support: files.len(),
            total: tests.len(),
            exemplars: ex.into_iter().take(3).collect(),
            counterexamples: Vec::new(),
        }]
    }
}

/// The most common choice of a dimension, if it dominates (≥ 60%).
fn dominant(
    repo: &str,
    dimension: &str,
    key: &str,
    choices: BTreeMap<String, Vec<String>>,
    generic: impl Fn(&str) -> String,
    unit: &str,
) -> Vec<Convention> {
    let total: usize = choices.values().map(Vec::len).sum();
    let Some((name, sites)) = choices.iter().max_by_key(|(_, v)| v.len()) else {
        return Vec::new();
    };
    if total < 3 || sites.len() * 10 < total * 6 {
        return Vec::new();
    }
    let deviants: Vec<String> = choices
        .iter()
        .filter(|(n, _)| *n != name)
        .flat_map(|(_, v)| v.iter().cloned())
        .take(3)
        .collect();
    let g = generic(name);
    vec![Convention {
        origin: Origin::Measured,
        repo: repo.into(),
        dimension: dimension.into(),
        key: format!("{key}:{name}"),
        rule: format!("{g} ({} of {total} {unit})", sites.len()),
        generic: g,
        support: sites.len(),
        total,
        exemplars: sites.iter().take(3).cloned().collect(),
        counterexamples: deviants,
    }]
}

/// `` `Name` (repo) `` for internal bases, `` `module.Name` `` for external ones.
fn label(name: &str, repo: Option<&str>) -> String {
    match repo {
        Some(r) => format!("`{name}` ({r})"),
        None => format!("`{name}`"),
    }
}

fn is_error_name(name: &str) -> bool {
    name.ends_with("Error") || name.ends_with("Exception")
}

fn loc(s: &Symbol) -> String {
    format!("{}:{}", s.file_path, s.range.start.line)
}

fn dir_of(path: &str) -> String {
    path.rsplit_once('/').map_or(".", |(d, _)| d).to_string()
}

pub fn is_test(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/tests/")
        || p.starts_with("tests/")
        || p.contains("/test/")
        || p.contains("test_")
        || p.contains("_test.")
        || p.contains(".test.")
        || p.contains(".spec.")
        || p.ends_with("conftest.py")
}

fn has_params(sig: &str) -> bool {
    let inner = sig
        .split_once('(')
        .and_then(|(_, r)| r.split_once(')'))
        .map_or("", |(a, _)| a);
    inner
        .split(',')
        .map(str::trim)
        .any(|p| !p.is_empty() && !matches!(p, "self" | "cls" | "this"))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// The first segment of a third-party import (`boto3`, `@scope/pkg`), or `None` if internal.
fn external_root(module: &str, internal: &HashSet<&str>) -> Option<String> {
    if module.is_empty() || module.starts_with('.') || internal_module(module, internal).is_some() {
        return None;
    }
    if module.starts_with('@') {
        return Some(module.splitn(3, '/').take(2).collect::<Vec<_>>().join("/"));
    }
    Some(module.split(['.', '/']).next()?.to_string())
}

/// The internal module an import names (longest known prefix).
fn internal_module<'m>(module: &str, internal: &HashSet<&'m str>) -> Option<&'m str> {
    let mut m = module;
    loop {
        if let Some(found) = internal.get(m) {
            return Some(found);
        }
        m = &m[..m.rfind(['.', '/'])?];
    }
}

/// `logging.getLogger` from a path whose head is bound by an import of a third-party module.
fn external_name(f: &FileFacts, path: &[String], internal: &HashSet<&str>) -> Option<String> {
    let (head, rest) = path.split_first()?;
    let base = f.imports.iter().find_map(|imp| {
        imp.items.iter().find_map(|item| match item {
            ImportItem::Name { name, alias } if alias.as_deref().unwrap_or(name) == head => {
                Some(format!("{}.{name}", imp.module))
            }
            ImportItem::Module { alias: Some(a) } if a == head => Some(imp.module.clone()),
            ImportItem::Module { alias: None }
                if imp.module.split('.').next() == Some(head.as_str()) =>
            {
                Some(head.clone())
            }
            _ => None,
        })
    })?;
    internal_module(&base, internal).is_none().then(|| {
        let mut parts = vec![base];
        parts.extend(rest.iter().cloned());
        parts.join(".")
    })
}

/// Like [`external_name`] for internal targets.
fn internal_name(f: &FileFacts, path: &[String]) -> Option<String> {
    let (head, rest) = path.split_first()?;
    let base = f.imports.iter().find_map(|imp| {
        imp.items.iter().find_map(|item| match item {
            ImportItem::Name { name, alias } if alias.as_deref().unwrap_or(name) == head => {
                Some(format!("{}.{name}", imp.module))
            }
            ImportItem::Module { alias: Some(a) } if a == head => Some(imp.module.clone()),
            _ => None,
        })
    })?;
    let mut parts = vec![base];
    parts.extend(rest.iter().cloned());
    Some(parts.join("."))
}

/// Identifier casing: `snake_case`, `camelCase`, `PascalCase`, `SCREAMING_SNAKE`.
pub fn casing(name: &str) -> &'static str {
    let core = name.trim_matches('_');
    if core.is_empty() {
        return "other";
    }
    let up = core.chars().any(|c| c.is_ascii_uppercase());
    let lo = core.chars().any(|c| c.is_ascii_lowercase());
    if !lo && up {
        return "SCREAMING_SNAKE";
    }
    if core.contains('_') && lo {
        return if core.starts_with(|c: char| c.is_ascii_uppercase()) {
            "other"
        } else {
            "snake_case"
        };
    }
    let f = core.chars().next().unwrap_or('a');
    if f.is_ascii_uppercase() && lo {
        return "PascalCase";
    }
    if f.is_ascii_lowercase() && up {
        return "camelCase";
    }
    "snake_case"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casing_classes() {
        assert_eq!(casing("get_user"), "snake_case");
        assert_eq!(casing("getUser"), "camelCase");
        assert_eq!(casing("UserService"), "PascalCase");
        assert_eq!(casing("MAX_SIZE"), "SCREAMING_SNAKE");
        assert_eq!(casing("_private_helper"), "snake_case");
    }

    #[test]
    fn org_baseline_uses_generic_rules() {
        let c = |repo: &str| Convention {
            origin: Origin::Measured,
            repo: repo.into(),
            dimension: "naming".into(),
            key: "naming:functions:snake_case".into(),
            rule: format!("Functions are snake_case (10 of {repo})"),
            generic: "Functions are snake_case".into(),
            support: 10,
            total: 10,
            exemplars: vec![format!("{repo}/a.py:1")],
            counterexamples: Vec::new(),
        };
        let org = org_baseline(&[c("a"), c("b")]);
        assert_eq!(org.len(), 1);
        assert!(org[0]
            .rule
            .starts_with("Functions are snake_case — in 2 of 2 repos"));
    }

    #[test]
    fn manual_changes_apply_and_undo() {
        let measured = vec![Convention {
            origin: Origin::Measured,
            repo: "svc".into(),
            dimension: "errors".into(),
            key: "errors:root:Base".into(),
            rule: "Errors derive from Base — 9 of 10".into(),
            generic: "Errors derive from Base".into(),
            support: 9,
            total: 10,
            exemplars: vec!["e.py:1".into()],
            counterexamples: Vec::new(),
        }];
        let id = measured[0].id();
        let added = Manual::new("svc", "Libraries", "Use the shared HTTP client", "t");
        let mut manual = vec![added.clone()];

        let edit = change(
            &apply(measured.clone(), &manual),
            &manual,
            &id,
            Change::Edit("Errors derive from Base".into()),
            "t",
        )
        .unwrap();
        let Write::Put(edit) = edit else { panic!() };
        assert_eq!(edit.replaces.as_deref(), Some("errors:root:Base"));
        manual.push(edit);
        let shown = apply(measured.clone(), &manual);
        assert_eq!(shown.len(), 2);
        assert_eq!(
            (shown[0].origin, shown[0].rule.as_str(), shown[0].support),
            (Origin::Edited, "Errors derive from Base", 9)
        );
        assert_eq!(
            (shown[1].origin, shown[1].dimension.as_str(), shown[1].id()),
            (Origin::Manual, "libraries", added.id.clone())
        );

        let Write::Put(hide) = change(&shown, &manual, &id, Change::Hide, "t").unwrap() else {
            panic!()
        };
        assert!(hide.hidden);
        manual.retain(|m| m.id != id);
        manual.push(hide);
        assert_eq!(apply(measured.clone(), &manual).len(), 1);

        let all = apply(measured.clone(), &manual);
        assert_eq!(
            change(&all, &manual, &id, Change::Restore, "t").unwrap(),
            Write::Delete(id.clone())
        );
        assert_eq!(
            change(&all, &manual, &added.id, Change::Hide, "t").unwrap(),
            Write::Delete(added.id.clone())
        );
        assert!(change(&all, &manual, &added.id, Change::Restore, "t").is_err());
    }
}
