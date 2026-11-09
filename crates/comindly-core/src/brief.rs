//! What the LLM is shown about a symbol or a flow: its code, docstring, place and neighbours.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::model::{Edge, EdgeKind, Symbol, SymbolKind};

const CODE_LINES: usize = 60;
const CODE_CHARS: usize = 3000;
const CALLS: usize = 8;
const CALLERS: usize = 6;

/// The indexed checkouts, read once per file.
pub struct Sources {
    roots: HashMap<String, PathBuf>,
    files: HashMap<(String, String), Option<String>>,
}

impl Sources {
    pub fn new(roots: &[(String, PathBuf)]) -> Sources {
        Sources {
            roots: roots.iter().cloned().collect(),
            files: HashMap::new(),
        }
    }

    /// Use `text` as the contents of `path` in `repo`, e.g. read from the index rather than a checkout.
    pub fn insert(&mut self, repo: &str, path: &str, text: Option<String>) {
        self.files.insert((repo.into(), path.into()), text);
    }

    /// The symbol's own lines, cut to a size an LLM call can carry.
    pub fn code(&mut self, s: &Symbol) -> Option<String> {
        let key = (s.repo.0.clone(), s.file_path.clone());
        let text = self
            .files
            .entry(key)
            .or_insert_with(|| {
                let root = self.roots.get(&s.repo.0)?;
                std::fs::read_to_string(root.join(&s.file_path)).ok()
            })
            .as_deref()?;
        let (start, end) = (s.range.start.line as usize, s.range.end.line as usize);
        let lines: Vec<&str> = text
            .lines()
            .skip(start.saturating_sub(1))
            .take(end.saturating_sub(start) + 1)
            .collect();
        if lines.iter().all(|l| l.trim().is_empty()) {
            return None;
        }
        let mut code = lines[..lines.len().min(CODE_LINES)].join("\n");
        if code.len() > CODE_CHARS {
            let cut = (0..=CODE_CHARS)
                .rev()
                .find(|&i| code.is_char_boundary(i))
                .unwrap_or(0);
            code.truncate(cut);
        }
        if lines.len() > CODE_LINES || code.len() == CODE_CHARS {
            code.push_str("\n… (cut)");
        }
        Some(code)
    }
}

/// Each symbol's edges in both directions, for naming its neighbours.
pub struct Neighbours<'a> {
    by_id: HashMap<String, &'a Symbol>,
    out: HashMap<String, Vec<(&'a EdgeKind, String)>>,
    into: HashMap<String, Vec<(&'a EdgeKind, String)>>,
}

impl<'a> Neighbours<'a> {
    pub fn new(symbols: &'a [Symbol], edges: &'a [Edge]) -> Neighbours<'a> {
        let mut n = Neighbours {
            by_id: symbols.iter().map(|s| (s.id.render(), s)).collect(),
            out: HashMap::new(),
            into: HashMap::new(),
        };
        for e in edges {
            let (src, dst) = (e.src.render(), e.dst.render());
            n.out
                .entry(src.clone())
                .or_default()
                .push((&e.kind, dst.clone()));
            n.into.entry(dst).or_default().push((&e.kind, src));
        }
        n
    }

    fn name(&self, id: &str, with_repo: bool) -> Option<String> {
        let s = self.by_id.get(id)?;
        Some(if with_repo {
            format!("{} ({})", s.name, s.repo.0)
        } else {
            s.name.clone()
        })
    }

    fn list(
        &self,
        edges: Option<&Vec<(&EdgeKind, String)>>,
        kind: &EdgeKind,
        max: usize,
        with_repo: bool,
    ) -> Option<String> {
        let mut names: Vec<String> = edges?
            .iter()
            .filter(|(k, _)| *k == kind)
            .filter_map(|(_, id)| self.name(id, with_repo))
            .collect();
        names.sort();
        names.dedup();
        if names.is_empty() {
            return None;
        }
        let more = names.len().saturating_sub(max);
        names.truncate(max);
        Some(match more {
            0 => names.join(", "),
            n => format!("{} and {n} more", names.join(", ")),
        })
    }
}

/// Everything the summary prompt sees about `s`.
pub fn symbol(s: &Symbol, n: &Neighbours, sources: &mut Sources) -> String {
    let id = s.id.render();
    let mut out = vec![
        format!("Symbol: {}", s.name),
        format!(
            "Kind: {:?} in repository {}, {}:{}",
            s.kind, s.repo.0, s.file_path, s.range.start.line
        ),
    ];
    let owner = n.into.get(&id).and_then(|v| {
        v.iter()
            .filter(|(k, _)| **k == EdgeKind::Contains)
            .filter_map(|(_, p)| n.by_id.get(p))
            .find(|p| !matches!(p.kind, SymbolKind::File | SymbolKind::Module))
    });
    if let Some(o) = owner {
        out.push(format!("Belongs to: {:?} {}", o.kind, o.name));
    }
    if let Some(sig) = s.signature.as_deref().filter(|x| !x.trim().is_empty()) {
        out.push(format!("Signature: {sig}"));
    }
    if let Some(doc) = s.docstring.as_deref().filter(|d| !d.trim().is_empty()) {
        out.push(format!("Docstring: {}", doc.trim()));
    }
    match sources.code(s) {
        Some(code) => out.push(format!("Code:\n```\n{code}\n```")),
        None => out.push("Code: not available".into()),
    }
    let outgoing = n.out.get(&id);
    if let Some(calls) = n.list(outgoing, &EdgeKind::Calls, CALLS, false) {
        out.push(format!("Calls: {calls}"));
    }
    if let Some(callers) = n.list(n.into.get(&id), &EdgeKind::Calls, CALLERS, true) {
        out.push(format!("Called by: {callers}"));
    }
    let contracts: Vec<String> = [
        EdgeKind::Serves,
        EdgeKind::Consumes,
        EdgeKind::Publishes,
        EdgeKind::Subscribes,
        EdgeKind::Reads,
        EdgeKind::Writes,
    ]
    .iter()
    .filter_map(|k| Some(format!("{k} {}", n.list(outgoing, k, CALLS, false)?)))
    .collect();
    if !contracts.is_empty() {
        out.push(format!("Contracts: {}", contracts.join("; ")));
    }
    out.join("\n")
}

/// A flow's trace followed by its entry point's code, when there is some.
pub fn flow(trace: String, entry: &Symbol, sources: &mut Sources) -> String {
    match sources.code(entry) {
        Some(code) => format!(
            "{trace}\n\nEntry point code ({}):\n```\n{code}\n```",
            entry.repo.0
        ),
        None => trace,
    }
}

/// The trace a walkthrough is written from: each step with its one-line summary when known.
pub fn trace(steps: &[crate::graph::FlowStep], summaries: &HashMap<String, String>) -> String {
    steps
        .iter()
        .map(|h| {
            let line = format!(
                "d{} {} {}:{} {}",
                h.depth, h.via, h.node.repo, h.node.name, h.node.location
            );
            match summaries.get(&h.node.id) {
                Some(s) => format!("{line} — {s}"),
                None => line,
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GlobalSymbolId, Language, Position, Range, RepoId};

    fn sym(name: &str, kind: SymbolKind, lines: (u32, u32)) -> Symbol {
        Symbol {
            id: GlobalSymbolId {
                scheme: "t".into(),
                package_manager: ".".into(),
                package: "shop".into(),
                version: ".".into(),
                descriptor: name.into(),
            },
            name: name.into(),
            kind,
            language: Language::Python,
            repo: RepoId("shop".into()),
            file_path: "shop/api.py".into(),
            range: Range {
                start: Position {
                    line: lines.0,
                    column: 0,
                },
                end: Position {
                    line: lines.1,
                    column: 0,
                },
            },
            signature: Some(format!("def {name}()")),
            docstring: None,
            fingerprint: None,
        }
    }

    fn edge(a: &Symbol, b: &Symbol, kind: EdgeKind) -> Edge {
        Edge {
            src: a.id.clone(),
            dst: b.id.clone(),
            kind,
            confidence: 1.0,
            cross_repo: false,
        }
    }

    #[test]
    fn a_symbol_brief_has_its_code_and_neighbours() {
        let dir = std::env::temp_dir().join(format!("comindly-brief-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("shop")).unwrap();
        std::fs::write(
            dir.join("shop/api.py"),
            "import db\n\ndef create():\n    db.save()\n\ndef handler():\n    create()\n",
        )
        .unwrap();
        let create = sym("create", SymbolKind::Function, (3, 4));
        let save = sym("save", SymbolKind::Function, (1, 1));
        let handler = sym("handler", SymbolKind::Function, (6, 7));
        let mut route = sym("http:POST /orders", SymbolKind::Contract, (1, 1));
        route.signature = None;
        let symbols = vec![create.clone(), save.clone(), handler.clone(), route.clone()];
        let edges = vec![
            edge(&create, &save, EdgeKind::Calls),
            edge(&handler, &create, EdgeKind::Calls),
            edge(&create, &route, EdgeKind::Serves),
        ];
        let n = Neighbours::new(&symbols, &edges);
        let mut sources = Sources::new(&[("shop".into(), dir.clone())]);
        let brief = symbol(&create, &n, &mut sources);
        std::fs::remove_dir_all(&dir).ok();
        assert!(brief.contains("def create():\n    db.save()"), "{brief}");
        assert!(
            !brief.contains("def handler"),
            "only its own lines: {brief}"
        );
        assert!(brief.contains("Calls: save"), "{brief}");
        assert!(brief.contains("Called by: handler (shop)"), "{brief}");
        assert!(
            brief.contains("Contracts: serves http:POST /orders"),
            "{brief}"
        );
    }
}
