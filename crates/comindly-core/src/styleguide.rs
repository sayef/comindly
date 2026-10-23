//! Evidence for the multi-pass LLM style-guide synthesis.
//!
//! A single prompt produces a shallow guide. Instead we split the guide into ~10 dimensions and
//! hand each its OWN tailored evidence (real signatures for the parameters section, the biggest
//! classes for design patterns, per-library call-sites for library usage, …), then run one focused
//! LLM call per dimension and stitch the results. The valuable, non-surface conventions live in the
//! code, so every section is grounded in real excerpts, dependency facts, or measured stats.
//! `COMINDLY_DEBUG_EVIDENCE=1` prints each section's evidence to stderr.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::model::{Edge, EdgeKind, Symbol, SymbolKind};

/// One dimension of the guide: a heading, guidance for the LLM, and the evidence it reasons over.
pub struct Section {
    pub title: String,
    pub guidance: String,
    pub evidence: String,
}

const EXCERPT_LINES: usize = 22;
const SECTION_BUDGET: usize = 9000; // per-section evidence cap (chars)

/// An evidence builder for one section.
type EvFn = fn(&Ctx) -> String;

/// Build every guide section for one repo. Reads source lazily (cached).
pub fn build_sections(
    root: &Path,
    symbols: &[&Symbol],
    edges: &[Edge],
    conventions: &[crate::conventions::Convention],
) -> Vec<Section> {
    let ctx = Ctx::new(root, symbols, edges);
    let specs: &[(&str, &str, EvFn)] = &[
        ("Stack & dependencies",
         "Name each key dependency and, in one line, what the repo uses it for. Note the language/runtime + versions. Group first-party (their own) modules separately. No generic filler.",
         ev_stack),
        ("Project layout & architecture",
         "Describe the directory responsibilities, the layering (e.g. routes→services→repositories→models), where entry points live, and how modules are wired. Cite real dirs/files. If layering isn't evident, say so.",
         ev_layout),
        ("Naming & formatting",
         "State the casing per symbol kind (with the measured ratio), private-name convention, file/module naming, line length / quote style / indent from the tooling config. Give a real conforming example. Bans name the replacement.",
         ev_naming),
        ("Function signatures & parameters",
         "From the real signatures: how are parameters typed, ordered, defaulted? keyword-only args, *args/**kwargs, return-type annotations, self/cls, async. Document the dominant signature shape with a real example; flag what to avoid.",
         ev_signatures),
        ("Types & data modeling",
         "How is data modeled and validated — pydantic BaseModel / dataclasses / TypedDict / TS interfaces / zod? Where do models live, how are they named, how is validation done at boundaries? Quote a real model.",
         ev_types),
        ("Design patterns & idioms",
         "Identify the recurring architectural patterns from the largest/most-central classes and decorated defs: base classes, dependency injection, factories, context managers, decorators, service/repository objects, the repo's own wrappers. Quote a real one and say when to follow it.",
         ev_patterns),
        ("Library & infra usage",
         "For each major dependency shown, document the ONE idiomatic way this repo uses it, quoting a real call-site (file path). Prefer the repo's own wrapper over the raw library where one exists (e.g. AWS via a wrapper, not raw boto3). Include DB/HTTP access patterns.",
         ev_library),
        ("Error handling, logging & configuration",
         "How are errors raised/caught (custom exceptions? broad except? Result types?), how is logging done (which logger, structured?), and how is configuration/secrets accessed (never hardcoded)? Quote real call-sites.",
         ev_errlog),
        ("Testing",
         "Framework, test file location/naming, fixtures, how external I/O is mocked, assertion style. Quote a real test. If tests are sparse, say so.",
         ev_testing),
        ("Docstrings & comments",
         "Dominant docstring style (Google/NumPy/reST/JSDoc) and when docs are expected (public API?), comment density/placement. Quote a real docstring. Don't demand docs the repo doesn't write.",
         ev_docstrings),
    ];
    specs
        .iter()
        .filter_map(|(title, guidance, f)| {
            let mut evidence = measured(&ctx, title, conventions);
            evidence.push_str(&f(&ctx));
            if evidence.trim().is_empty() {
                return None;
            }
            if evidence.len() > SECTION_BUDGET {
                evidence.truncate(SECTION_BUDGET);
                evidence.push_str("\n…(truncated)");
            }
            Some(Section {
                title: title.to_string(),
                guidance: guidance.to_string(),
                evidence,
            })
        })
        .collect()
}

/// Measured conventions for a section's dimensions, with excerpts at their exemplar locations.
fn measured(ctx: &Ctx, title: &str, conventions: &[crate::conventions::Convention]) -> String {
    let dims: &[&str] = match title {
        "Stack & dependencies" => &["libraries"],
        "Project layout & architecture" => &["layout"],
        "Naming & formatting" => &["naming"],
        "Function signatures & parameters" => &["signatures"],
        "Types & data modeling" | "Design patterns & idioms" => &["models"],
        "Library & infra usage" => &["libraries", "config"],
        "Error handling, logging & configuration" => &["errors", "logging", "config"],
        "Testing" => &["testing"],
        _ => &[],
    };
    let relevant: Vec<_> = conventions
        .iter()
        .filter(|c| dims.contains(&c.dimension.as_str()))
        .collect();
    if relevant.is_empty() {
        return String::new();
    }
    let mut o =
        String::from("Measured conventions (authoritative: cite them, never contradict them):\n");
    for c in &relevant {
        let _ = writeln!(o, "- {}", c.rule);
        if !c.counterexamples.is_empty() {
            let _ = writeln!(o, "  deviations: {}", c.counterexamples.join(", "));
        }
    }
    for c in relevant.iter().take(4) {
        for ex in c.exemplars.iter().take(1) {
            let Some((file, line)) = ex.rsplit_once(':') else {
                continue;
            };
            let start = line.parse::<usize>().unwrap_or(1);
            if let Some(body) = ctx.excerpt(file, start, 14) {
                block(&mut o, &format!("{ex} — exemplar of: {}", c.generic), &body);
            }
        }
    }
    o.push('\n');
    o
}

// ---- shared repo context -------------------------------------------------------------------
struct Ctx<'a> {
    root: PathBuf,
    symbols: &'a [&'a Symbol],
    files: Vec<String>,
    cache: RefCell<HashMap<String, Option<String>>>,
    hist: Vec<(String, Vec<String>)>, // module -> importing files, ranked
}

impl<'a> Ctx<'a> {
    fn new(root: &Path, symbols: &'a [&'a Symbol], edges: &'a [Edge]) -> Self {
        let mut files: Vec<String> = symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::File))
            .map(|s| s.file_path.clone())
            .collect();
        files.sort();
        files.dedup();
        let id2file: HashMap<String, String> = symbols
            .iter()
            .map(|s| (s.id.render(), s.file_path.clone()))
            .collect();
        let mut mods: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for e in edges.iter().filter(|e| e.kind == EdgeKind::Imports) {
            let d = &e.dst.descriptor;
            let p: Vec<&str> = d.split('/').collect();
            let m = if d.starts_with('@') && p.len() >= 2 {
                format!("{}/{}", p[0], p[1])
            } else {
                p.first().unwrap_or(&"").to_string()
            };
            if m.is_empty() {
                continue;
            }
            let f = id2file.get(&e.src.render()).cloned().unwrap_or_default();
            if !f.is_empty() {
                mods.entry(m).or_default().insert(f);
            }
        }
        let mut hist: Vec<(String, Vec<String>)> = mods
            .into_iter()
            .map(|(m, f)| (m, f.into_iter().collect()))
            .collect();
        hist.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        Ctx {
            root: root.to_path_buf(),
            symbols,
            files,
            cache: RefCell::new(HashMap::new()),
            hist,
        }
    }

    fn read(&self, file: &str) -> Option<String> {
        if let Some(v) = self.cache.borrow().get(file) {
            return v.clone();
        }
        let v = std::fs::read_to_string(self.root.join(file)).ok();
        self.cache.borrow_mut().insert(file.to_string(), v.clone());
        v
    }

    fn excerpt(&self, file: &str, start: usize, n: usize) -> Option<String> {
        let src = self.read(file)?;
        let lines: Vec<&str> = src.lines().skip(start.saturating_sub(1)).take(n).collect();
        if lines.iter().all(|l| l.trim().is_empty()) {
            return None;
        }
        Some(lines.join("\n"))
    }

    fn syms(&self, kinds: &[SymbolKind]) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| kinds.contains(&s.kind))
            .copied()
            .collect()
    }

    /// Find files whose content matches a predicate (bounded scan).
    fn files_matching(&self, pred: impl Fn(&str) -> bool, limit: usize) -> Vec<String> {
        let mut out = Vec::new();
        for f in &self.files {
            if out.len() >= limit {
                break;
            }
            if let Some(src) = self.read(f) {
                if pred(&src) {
                    out.push(f.clone());
                }
            }
        }
        out
    }
}

fn block(o: &mut String, file: &str, body: &str) {
    let _ = writeln!(o, "\n### {file}\n```\n{}\n```", body.trim_end());
}

// ---- casing (shared) -----------------------------------------------------------------------
fn casing(name: &str) -> &'static str {
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
        return "snake_case";
    }
    let f = core.chars().next().unwrap();
    if f.is_ascii_uppercase() && lo {
        return "PascalCase";
    }
    if f.is_ascii_lowercase() && up {
        return "camelCase";
    }
    "snake_case"
}

// ---- section evidence builders -------------------------------------------------------------
fn ev_stack(ctx: &Ctx) -> String {
    let mut o = String::new();
    let read = |n: &str| std::fs::read_to_string(ctx.root.join(n)).ok();
    let mut deps: Vec<String> = Vec::new();
    if let Some(t) = read("pyproject.toml") {
        if let Ok(v) = t.parse::<toml::Value>() {
            if let Some(a) = v
                .get("project")
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_array())
            {
                deps.extend(a.iter().filter_map(|d| d.as_str().map(str::to_string)));
            }
        }
    }
    if let Some(t) = read("package.json") {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&t) {
            if let Some(ob) = v.get("dependencies").and_then(|d| d.as_object()) {
                deps.extend(
                    ob.iter()
                        .map(|(k, ver)| format!("{k} {}", ver.as_str().unwrap_or(""))),
                );
            }
        }
    }
    if !deps.is_empty() {
        let _ = writeln!(o, "Declared dependencies:");
        for d in deps.iter().take(40) {
            let _ = writeln!(o, "- {}", d.trim());
        }
    }
    if !ctx.hist.is_empty() {
        let _ = writeln!(
            o,
            "\nMost-imported modules (module → #files; some are first-party):"
        );
        for (m, f) in ctx.hist.iter().take(20) {
            let _ = writeln!(o, "- {m}  ({} files)", f.len());
        }
    }
    o
}

fn ev_layout(ctx: &Ctx) -> String {
    let mut o = String::new();
    let mut dirs: BTreeMap<String, usize> = BTreeMap::new();
    for f in &ctx.files {
        let d = match f.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => "(root)".into(),
        };
        *dirs.entry(d).or_default() += 1;
    }
    let _ = writeln!(o, "Directories (path → file count):");
    for (d, n) in dirs.iter().take(40) {
        let _ = writeln!(o, "- {d}  {n}");
    }
    // entry points + a couple central files
    let hints = [
        "main.py",
        "app.py",
        "cli.py",
        "index.ts",
        "index.tsx",
        "__main__.py",
        "server.py",
    ];
    let mut shown = 0;
    let _ = writeln!(o, "\nEntry-point / central files:");
    for f in &ctx.files {
        if hints.iter().any(|h| f.ends_with(h)) {
            if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
                block(&mut o, f, &ex);
                shown += 1;
            }
        }
        if shown >= 4 {
            break;
        }
    }
    o
}

fn ev_naming(ctx: &Ctx) -> String {
    let mut o = String::new();
    let dom = |kinds: &[SymbolKind]| {
        let mut h: BTreeMap<&str, usize> = BTreeMap::new();
        let ss = ctx.syms(kinds);
        for s in &ss {
            *h.entry(casing(&s.name)).or_default() += 1;
        }
        let total = ss.len();
        let names: Vec<String> = ss.iter().take(10).map(|s| s.name.clone()).collect();
        (
            h.into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(k, c)| (k.to_string(), c)),
            total,
            names,
        )
    };
    for (label, kinds) in [
        (
            "functions/methods",
            &[SymbolKind::Function, SymbolKind::Method][..],
        ),
        (
            "classes/interfaces",
            &[SymbolKind::Class, SymbolKind::Interface][..],
        ),
    ] {
        let (d, total, names) = dom(kinds);
        if let Some((k, c)) = d {
            let _ = writeln!(
                o,
                "{label}: {k} {c}/{total}. examples: {}",
                names.join(", ")
            );
        }
    }
    // formatting facts
    if let Ok(t) = std::fs::read_to_string(ctx.root.join(".editorconfig")) {
        for l in t.lines() {
            let l = l.trim();
            if ["indent_style", "indent_size", "max_line_length"]
                .iter()
                .any(|k| l.starts_with(k))
            {
                let _ = writeln!(o, "editorconfig: {l}");
            }
        }
    }
    if let Ok(t) = std::fs::read_to_string(ctx.root.join("pyproject.toml")) {
        for key in ["line-length", "quote-style"] {
            if let Some(line) = t.lines().find(|l| l.trim_start().starts_with(key)) {
                let _ = writeln!(o, "ruff/black: {}", line.trim());
            }
        }
    }
    o
}

fn ev_signatures(ctx: &Ctx) -> String {
    let mut o = String::from("Real signatures (sampled):\n");
    let mut n = 0;
    for s in ctx.syms(&[SymbolKind::Function, SymbolKind::Method]) {
        if let Some(sig) = &s.signature {
            let sig = sig.trim();
            if sig.len() > 6 {
                let _ = writeln!(o, "- {}  ({}:{})", sig, s.file_path, s.range.start.line);
                n += 1;
            }
        }
        if n >= 30 {
            break;
        }
    }
    if n == 0 {
        return String::new();
    }
    o
}

fn ev_types(ctx: &Ctx) -> String {
    let files = ctx.files_matching(
        |s| {
            s.contains("BaseModel")
                || s.contains("@dataclass")
                || s.contains("TypedDict")
                || s.contains("z.object(")
                || s.contains("interface ")
                || s.contains("z.infer")
        },
        4,
    );
    if files.is_empty() {
        return String::new();
    }
    let mut o = String::from("Model / type definitions:\n");
    for f in &files {
        if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
            block(&mut o, f, &ex);
        }
    }
    o
}

fn ev_patterns(ctx: &Ctx) -> String {
    let mut o = String::new();
    // largest classes (by line span) — usually base/service/controller classes
    let mut classes = ctx.syms(&[SymbolKind::Class]);
    classes.sort_by_key(|s| std::cmp::Reverse(s.range.end.line.saturating_sub(s.range.start.line)));
    if !classes.is_empty() {
        let _ = writeln!(o, "Largest classes (likely base/service/controller):");
    }
    for s in classes.iter().take(4) {
        if let Some(ex) = ctx.excerpt(&s.file_path, s.range.start.line as usize, EXCERPT_LINES) {
            block(
                &mut o,
                &format!("{} ({}:{})", s.name, s.file_path, s.range.start.line),
                &ex,
            );
        }
    }
    // decorated defs (DI / factories / context managers)
    let deco = ctx.files_matching(
        |src| {
            src.contains("@contextmanager")
                || src.contains("@property")
                || src.contains("@staticmethod")
                || src.contains("@classmethod")
                || src.contains("Depends(")
                || src.contains("@inject")
        },
        2,
    );
    for f in &deco {
        if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
            block(&mut o, f, &ex);
        }
    }
    o
}

fn ev_library(ctx: &Ctx) -> String {
    let mut o = String::from("Representative call-sites per major dependency:\n");
    let mut shown = 0;
    for (m, files) in ctx.hist.iter().take(10) {
        if let Some(f) = files.first() {
            if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
                block(&mut o, &format!("{f}  (uses `{m}`)"), &ex);
                shown += 1;
            }
        }
        if shown >= 7 {
            break;
        }
    }
    if shown == 0 {
        return String::new();
    }
    o
}

fn ev_errlog(ctx: &Ctx) -> String {
    let files = ctx.files_matching(
        |s| {
            s.contains("except ")
                || s.contains("logger")
                || s.contains("getLogger")
                || s.contains("raise ")
                || s.contains("get_config")
                || s.contains("Settings(")
        },
        4,
    );
    if files.is_empty() {
        return String::new();
    }
    let mut o = String::from("Error / logging / config call-sites:\n");
    for f in &files {
        if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
            block(&mut o, f, &ex);
        }
    }
    o
}

fn ev_testing(ctx: &Ctx) -> String {
    let files: Vec<String> = ctx
        .files
        .iter()
        .filter(|f| {
            let l = f.to_lowercase();
            l.contains("test_")
                || l.ends_with("_test.py")
                || l.ends_with(".test.ts")
                || l.ends_with(".spec.ts")
                || l.contains("/tests/")
                || l.ends_with("conftest.py")
        })
        .take(3)
        .cloned()
        .collect();
    if files.is_empty() {
        return String::new();
    }
    let mut o = String::from("Test files:\n");
    for f in &files {
        if let Some(ex) = ctx.excerpt(f, 1, EXCERPT_LINES) {
            block(&mut o, f, &ex);
        }
    }
    o
}

fn ev_docstrings(ctx: &Ctx) -> String {
    let (mut g, mut np, mut rest, mut jsdoc) = (0, 0, 0, 0);
    for f in ctx.files.iter().take(400) {
        if let Some(s) = ctx.read(f) {
            if s.contains("Args:") || s.contains("Returns:") {
                g += 1;
            }
            if s.contains("Parameters\n") {
                np += 1;
            }
            if s.contains(":param ") {
                rest += 1;
            }
            if s.contains("@param") || s.contains("/**") {
                jsdoc += 1;
            }
        }
    }
    let style = [
        ("Google", g),
        ("NumPy", np),
        ("reST", rest),
        ("JSDoc", jsdoc),
    ]
    .into_iter()
    .max_by_key(|(_, n)| *n)
    .filter(|(_, n)| *n > 0)
    .map(|(k, _)| k);
    let style = match style {
        Some(s) => s,
        None => return String::new(),
    };
    let mut o = format!("Dominant docstring style: {style}.\nExamples of documented code:\n");
    // a couple of files that actually contain the style marker
    let marker = match style {
        "Google" => "Returns:",
        "NumPy" => "Parameters\n",
        "reST" => ":param ",
        _ => "/**",
    };
    for f in ctx.files_matching(|s| s.contains(marker), 2) {
        if let Some(ex) = ctx.excerpt(&f, 1, EXCERPT_LINES + 8) {
            block(&mut o, &f, &ex);
        }
    }
    o
}
