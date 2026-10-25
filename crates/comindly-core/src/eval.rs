//! Graph-quality evaluation: compare resolved edges against a gold set (`expected.toml`).
//!
//! A fixture dir holds one subdirectory per repo plus `expected.toml`. Edges are keyed by
//! `repo:path` (file) or `repo:path::Qualified.name`, independent of the descriptor scheme.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::{Edge, EdgeKind, Symbol, SymbolKind};

/// Edge kinds the gold set covers; other kinds are ignored on both sides.
pub const EVAL_KINDS: &[&str] = &[
    "imports",
    "calls",
    "inherits",
    "implements",
    "instantiates",
    "serves",
    "consumes",
    "publishes",
    "subscribes",
    "reads",
    "writes",
    "defines",
];

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct GoldEdge {
    pub case: String,
    pub kind: String,
    pub src: String,
    pub dst: String,
}

#[derive(Debug, Deserialize)]
struct Gold {
    edge: Vec<GoldEdge>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
}

impl Counts {
    pub fn precision(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }
    pub fn recall(&self) -> f64 {
        ratio(self.tp, self.tp + self.fn_)
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 {
        1.0
    } else {
        a as f64 / b as f64
    }
}

/// A produced edge, in gold-key form: (kind, src, dst).
pub type KeyedEdge = (String, String, String);

#[derive(Debug, Default)]
pub struct EvalReport {
    pub total: Counts,
    pub by_kind: BTreeMap<String, Counts>,
    /// case -> (found, expected)
    pub by_case: BTreeMap<String, (usize, usize)>,
    pub missed: Vec<GoldEdge>,
    pub spurious: Vec<KeyedEdge>,
    /// References answered by SCIP rather than tree-sitter resolution.
    pub scip_answers: usize,
}

/// Parse + resolve every repo under `fixture` and score the result against its `expected.toml`.
pub fn eval_fixture(fixture: &Path) -> Result<EvalReport> {
    eval_with(fixture, None, None)
}

/// Like [`eval_fixture`], limited to `only` repos (gold filtered to edges among them), with
/// `<scip>/<repo>.scip` indexes attached where present.
pub fn eval_with(fixture: &Path, only: Option<&[&str]>, scip: Option<&Path>) -> Result<EvalReport> {
    let gold_path = fixture.join("expected.toml");
    let gold: Gold = toml::from_str(
        &std::fs::read_to_string(&gold_path)
            .with_context(|| format!("read {}", gold_path.display()))?,
    )
    .with_context(|| format!("parse {}", gold_path.display()))?;

    let keep = |name: &str| only.is_none_or(|o| o.contains(&name));
    let mut repos: Vec<_> = std::fs::read_dir(fixture)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .collect();
    repos.sort_by_key(|e| e.file_name());

    let mut symbols = Vec::new();
    let mut edges = Vec::new();
    let mut facts = Vec::new();
    for r in repos {
        let name = r.file_name().to_string_lossy().to_string();
        let mut out = crate::parse::parse_repo(&r.path(), &name)?;
        if let Some(index) = scip.map(|d| d.join(format!("{name}.scip"))) {
            if index.is_file() {
                crate::scip::attach(&index, "", &name, &out.symbols, &mut out.facts)?;
            }
        }
        symbols.extend(out.symbols);
        edges.extend(out.edges);
        facts.extend(out.facts);
    }
    let resolved = crate::resolve::resolve(&symbols, &edges, &facts);
    symbols.extend(resolved.symbols.iter().cloned());
    let repo_of = |k: &str| k.split(':').next().unwrap_or_default().to_string();
    let gold: Vec<GoldEdge> = gold
        .edge
        .into_iter()
        .filter(|g| {
            keep(&repo_of(&g.src)) && (g.dst.starts_with("contract:") || keep(&repo_of(&g.dst)))
        })
        .collect();
    let mut report = score(&gold, &keyed_edges(&symbols, &resolved.edges));
    report.scip_answers = resolved.stats.scip_answers;
    Ok(report)
}

/// Render resolved edges of the evaluated kinds as gold keys.
pub fn keyed_edges(symbols: &[Symbol], edges: &[Edge]) -> BTreeSet<KeyedEdge> {
    let by_id: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.render(), s)).collect();
    let parent: HashMap<String, String> = edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Contains)
        .map(|e| (e.dst.render(), e.src.render()))
        .collect();
    let key = |id: &str| -> String {
        let Some(sym) = by_id.get(id) else {
            return format!("?{id}");
        };
        if sym.kind == SymbolKind::Contract {
            return format!("contract:{}", sym.name);
        }
        let mut names = Vec::new();
        let mut cur = Some(id.to_string());
        while let Some(c) = cur {
            match by_id.get(&c) {
                Some(s) if s.kind != SymbolKind::File => names.push(s.name.clone()),
                _ => break,
            }
            cur = parent.get(&c).cloned();
        }
        names.reverse();
        if names.is_empty() {
            format!("{}:{}", sym.repo.0, sym.file_path)
        } else {
            format!("{}:{}::{}", sym.repo.0, sym.file_path, names.join("."))
        }
    };
    edges
        .iter()
        .filter(|e| EVAL_KINDS.contains(&e.kind.as_str()))
        .map(|e| {
            (
                e.kind.as_str().to_string(),
                key(&e.src.render()),
                key(&e.dst.render()),
            )
        })
        .collect()
}

pub fn score(gold: &[GoldEdge], produced: &BTreeSet<KeyedEdge>) -> EvalReport {
    let mut r = EvalReport::default();
    let gold_keys: BTreeSet<KeyedEdge> = gold
        .iter()
        .map(|g| (g.kind.clone(), g.src.clone(), g.dst.clone()))
        .collect();
    for g in gold {
        let hit = produced.contains(&(g.kind.clone(), g.src.clone(), g.dst.clone()));
        let c = r.by_kind.entry(g.kind.clone()).or_default();
        let case = r.by_case.entry(g.case.clone()).or_default();
        case.1 += 1;
        if hit {
            c.tp += 1;
            case.0 += 1;
        } else {
            c.fn_ += 1;
            r.missed.push(g.clone());
        }
    }
    for p in produced.difference(&gold_keys) {
        r.by_kind.entry(p.0.clone()).or_default().fp += 1;
        r.spurious.push(p.clone());
    }
    for c in r.by_kind.values() {
        r.total.tp += c.tp;
        r.total.fp += c.fp;
        r.total.fn_ += c.fn_;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_counts_hits_misses_and_spurious() {
        let gold = vec![
            GoldEdge {
                case: "a".into(),
                kind: "calls".into(),
                src: "r:x.py::f".into(),
                dst: "r:x.py::g".into(),
            },
            GoldEdge {
                case: "b".into(),
                kind: "imports".into(),
                src: "r:x.py".into(),
                dst: "r:y.py::h".into(),
            },
        ];
        let produced: BTreeSet<KeyedEdge> = [
            ("calls".into(), "r:x.py::f".into(), "r:x.py::g".into()),
            ("calls".into(), "r:x.py::f".into(), "r:x.py::zzz".into()),
        ]
        .into_iter()
        .collect();
        let r = score(&gold, &produced);
        assert_eq!((r.total.tp, r.total.fp, r.total.fn_), (1, 1, 1));
        assert_eq!(r.by_case["a"], (1, 1));
        assert_eq!(r.by_case["b"], (0, 1));
    }
}
