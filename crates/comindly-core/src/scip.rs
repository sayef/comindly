//! SCIP ingestion: compiler-grade definitions and references attached to per-file facts.
//!
//! The resolver consults these first; tree-sitter resolution covers what SCIP doesn't.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use protobuf::Message;

use crate::facts::{FileFacts, ScipFacts, ScipRef};
use crate::model::{Symbol, SymbolKind};

const DEFINITION: i32 = 1;

/// How `index`/`link` use SCIP: ingest/run when available, insist, or skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Require,
    Off,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "auto" => Some(Mode::Auto),
            "require" => Some(Mode::Require),
            "off" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// Outcome of attaching SCIP data to one repo.
#[derive(Debug, Default)]
pub struct Attached {
    pub indexes: usize,
    pub documents: usize,
    pub definitions: usize,
    pub references: usize,
}

/// Find or produce SCIP indexes for `root` and attach them to this repo's facts.
pub fn ingest(
    mode: Mode,
    root: &Path,
    repo: &str,
    symbols: &[Symbol],
    facts: &mut [FileFacts],
) -> Result<Attached> {
    let mut out = Attached::default();
    if mode == Mode::Off {
        return Ok(out);
    }
    let mut indexes = find_indexes(root);
    let tmp = std::env::temp_dir().join(format!("comindly-scip-{}-{repo}", std::process::id()));
    if indexes.is_empty() {
        indexes = run_indexers(root, &tmp);
    }
    for (path, prefix) in &indexes {
        let a = attach(path, prefix, repo, symbols, facts)
            .with_context(|| format!("read {}", path.display()))?;
        out.indexes += 1;
        out.documents += a.documents;
        out.definitions += a.definitions;
        out.references += a.references;
    }
    std::fs::remove_dir_all(&tmp).ok();
    if mode == Mode::Require && out.documents == 0 {
        anyhow::bail!("{repo}: no SCIP index found or produced (scip = \"require\")");
    }
    Ok(out)
}

/// `index.scip` files under `root` (gitignored ones included), with their repo-relative dir.
fn find_indexes(root: &Path) -> Vec<(PathBuf, String)> {
    ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .max_depth(Some(4))
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some("node_modules" | ".git" | "target" | ".venv" | "venv" | "dist")
            )
        })
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name() == "index.scip")
        .filter_map(|e| {
            let dir = e.path().parent()?.strip_prefix(root).ok()?;
            Some((
                e.path().to_path_buf(),
                dir.to_string_lossy().replace('\\', "/"),
            ))
        })
        .collect()
}

/// Run the SCIP indexers on `PATH` that match the repo's manifests; returns produced indexes.
fn run_indexers(root: &Path, tmp: &Path) -> Vec<(PathBuf, String)> {
    let has = |f: &str| root.join(f).is_file();
    let mut jobs: Vec<(&str, Vec<String>)> = Vec::new();
    if has("tsconfig.json") || has("package.json") {
        jobs.push((
            "scip-typescript",
            vec!["index".into(), "--infer-tsconfig".into()],
        ));
    }
    if has("pyproject.toml") || has("setup.py") || has("requirements.txt") {
        jobs.push(("scip-python", vec!["index".into(), ".".into()]));
    }
    if has("go.mod") {
        jobs.push(("scip-go", Vec::new()));
    }
    if has("Cargo.toml") {
        jobs.push(("rust-analyzer", vec!["scip".into(), ".".into()]));
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        jobs.push(("scip-java", vec!["index".into()]));
    }
    let mut out = Vec::new();
    for (i, (bin, mut args)) in jobs.into_iter().enumerate() {
        if which(bin).is_none() {
            continue;
        }
        if std::fs::create_dir_all(tmp).is_err() {
            break;
        }
        let file = tmp.join(format!("{i}-{bin}.scip"));
        args.push("--output".into());
        args.push(file.to_string_lossy().to_string());
        let ok = Command::new(bin)
            .args(&args)
            .current_dir(root)
            .output()
            .is_ok_and(|o| o.status.success());
        if ok && file.is_file() {
            out.push((file, String::new()));
        }
    }
    out
}

pub fn which(bin: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// Attach one index's occurrences to facts of `repo`; document paths are relative to `prefix`.
pub fn attach(
    index: &Path,
    prefix: &str,
    repo: &str,
    symbols: &[Symbol],
    facts: &mut [FileFacts],
) -> Result<Attached> {
    let bytes = std::fs::read(index)?;
    let idx = scip::types::Index::parse_from_bytes(&bytes).context("decode SCIP index")?;

    let mut by_file: HashMap<&str, Vec<&Symbol>> = HashMap::new();
    for s in symbols
        .iter()
        .filter(|s| s.repo.0 == repo && !matches!(s.kind, SymbolKind::File))
    {
        by_file.entry(s.file_path.as_str()).or_default().push(s);
    }
    let mut slots: HashMap<String, usize> = HashMap::new();
    for (i, f) in facts.iter().enumerate().filter(|(_, f)| f.repo == repo) {
        slots.insert(f.path.clone(), i);
    }

    let mut out = Attached::default();
    for doc in &idx.documents {
        let path = if prefix.is_empty() {
            doc.relative_path.clone()
        } else {
            format!("{prefix}/{}", doc.relative_path)
        };
        let Some(&slot) = slots.get(&path) else {
            continue;
        };
        out.documents += 1;
        let defs_here = by_file.get(path.as_str()).cloned().unwrap_or_default();
        let sf = facts[slot].scip.get_or_insert_with(ScipFacts::default);
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() || scip::symbol::is_local_symbol(&occ.symbol) {
                continue;
            }
            let (Some(&line0), Some(name)) = (occ.range.first(), last_name(&occ.symbol)) else {
                continue;
            };
            let line = line0 as u32 + 1;
            if occ.symbol_roles & DEFINITION != 0 {
                // Innermost definition with this name spanning the line.
                if let Some(s) = defs_here
                    .iter()
                    .filter(|s| {
                        s.name == name && s.range.start.line <= line && line <= s.range.end.line
                    })
                    .max_by_key(|s| s.range.start.line)
                {
                    sf.defs.push((occ.symbol.clone(), s.id.render()));
                    out.definitions += 1;
                }
            } else {
                sf.refs.push(ScipRef {
                    line,
                    name,
                    symbol: occ.symbol.clone(),
                });
                out.references += 1;
            }
        }
    }
    Ok(out)
}

/// The last descriptor's name: `npm pkg 1.0 src/`a.ts`/Foo#bar().` → `bar`.
fn last_name(symbol: &str) -> Option<String> {
    let s = scip::symbol::parse_symbol(symbol).ok()?;
    s.descriptors.last().map(|d| d.name.clone())
}
