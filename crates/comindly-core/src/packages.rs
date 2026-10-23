//! Importable module names for repo files: Python source roots (`pyproject.toml`, `src/` layout)
//! and TS/JS packages (`package.json` name + entry point).

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::model::Language;

const TS_EXTS: &[&str] = &[".ts", ".tsx", ".d.ts", ".js", ".jsx", ".mjs", ".cjs"];

#[derive(Debug, Default, Clone)]
pub struct RepoPackages {
    root: PathBuf,
    /// tsconfig/jsconfig path mappings, deepest directory first.
    tsconfigs: Vec<TsConfig>,
    /// Repo-relative Python source roots, longest first; `""` is the repo root.
    py_roots: Vec<String>,
    /// TS/JS packages, deepest directory first.
    ts_pkgs: Vec<TsPackage>,
    /// Go modules as (repo-relative dir, module path), deepest first.
    go_mods: Vec<(String, String)>,
    /// Rust crates as (repo-relative dir, crate name with `_`), deepest first.
    crates: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct TsConfig {
    /// Repo-relative directory of the tsconfig (scope of files it applies to).
    dir: String,
    /// Repo-relative directory `paths` and bare specifiers resolve against.
    base: Option<String>,
    paths: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone)]
struct TsPackage {
    /// Repo-relative package directory (`""` for the repo root).
    dir: String,
    name: String,
    /// Repo-relative entry file, if it exists in the source tree.
    entry: Option<String>,
}

impl RepoPackages {
    /// Scan a repo for `pyproject.toml` and `package.json` manifests.
    pub fn discover(root: &Path) -> Self {
        let mut py_roots = vec![String::new()];
        let mut ts_pkgs = Vec::new();
        let mut tsconfigs = Vec::new();
        let mut go_mods = Vec::new();
        let mut crates = Vec::new();
        let manifests = WalkBuilder::new(root)
            .hidden(false)
            .filter_entry(|e| e.file_name() != "node_modules")
            .build()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let n = e.file_name();
                n == "pyproject.toml"
                    || n == "package.json"
                    || n == "tsconfig.json"
                    || n == "jsconfig.json"
                    || n == "go.mod"
                    || n == "Cargo.toml"
            });
        for m in manifests {
            let Ok(rel_dir) = m.path().parent().unwrap_or(root).strip_prefix(root) else {
                continue;
            };
            let dir = rel_dir.to_string_lossy().replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(m.path()) else {
                continue;
            };
            if m.file_name() == "pyproject.toml" {
                py_roots.extend(python_roots(root, &dir, &text));
            } else if m.file_name() == "Cargo.toml" {
                if let Some(name) = text
                    .parse::<toml::Table>()
                    .ok()
                    .and_then(|v| v.get("package")?.get("name")?.as_str().map(str::to_string))
                {
                    crates.push((dir, name.replace('-', "_")));
                }
            } else if m.file_name() == "go.mod" {
                if let Some(module) = text.lines().find_map(|l| l.trim().strip_prefix("module ")) {
                    go_mods.push((dir, module.trim().trim_matches('"').to_string()));
                }
            } else if m.file_name() == "package.json" {
                ts_pkgs.extend(ts_package(root, &dir, &text));
            } else {
                tsconfigs.extend(ts_config(root, &dir, &text));
            }
        }
        if root.join("src").is_dir() {
            py_roots.push("src".into());
        }
        py_roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
        py_roots.dedup();
        ts_pkgs.sort_by_key(|p| std::cmp::Reverse(p.dir.len()));
        tsconfigs.sort_by_key(|c: &TsConfig| std::cmp::Reverse(c.dir.len()));
        go_mods.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
        crates.sort_by_key(|(d, _)| std::cmp::Reverse(d.len()));
        RepoPackages {
            root: root.to_path_buf(),
            go_mods,
            crates,
            tsconfigs,
            py_roots,
            ts_pkgs,
        }
    }

    /// The repo file a non-relative TS specifier maps to via the governing tsconfig.
    pub fn ts_alias(&self, from: &str, spec: &str) -> Option<String> {
        let cfg = self.tsconfigs.iter().find(|c| under(from, &c.dir))?;
        let base = cfg.base.as_deref().unwrap_or(&cfg.dir);
        for (pat, targets) in &cfg.paths {
            let star = match pat.split_once('*') {
                Some((pre, post)) => spec
                    .strip_prefix(pre)
                    .and_then(|r| r.strip_suffix(post))
                    .map(str::to_string),
                None => (pat == spec).then(String::new),
            };
            let Some(star) = star else { continue };
            for t in targets {
                if let Some(f) = self.existing(&join(base, &t.replace('*', &star))) {
                    return Some(f);
                }
            }
        }
        cfg.base
            .as_deref()
            .and_then(|b| self.existing(&join(b, spec)))
    }

    /// The source file a module path denotes (extension or `/index` added), if it exists.
    fn existing(&self, stem: &str) -> Option<String> {
        let stem = strip_ts_ext(stem);
        [
            ".ts",
            ".tsx",
            ".d.ts",
            ".js",
            ".jsx",
            ".mjs",
            "/index.ts",
            "/index.tsx",
            "/index.js",
        ]
        .iter()
        .map(|e| format!("{stem}{e}"))
        .find(|p| self.root.join(p).is_file())
    }

    /// Names a file is importable by; empty when it belongs to no known package.
    pub fn module_names(&self, rel: &str, lang: &Language) -> Vec<String> {
        match lang {
            Language::Python => self.python_module(rel).into_iter().collect(),
            Language::TypeScript | Language::JavaScript => self.ts_specifiers(rel),
            Language::Go => self.go_package(rel).into_iter().collect(),
            Language::Rust => self.rust_module(rel).into_iter().collect(),
            _ => Vec::new(),
        }
    }

    fn python_module(&self, rel: &str) -> Option<String> {
        let root = self.py_roots.iter().find(|r| under(rel, r))?;
        let tail = rel.strip_prefix(root.as_str())?.trim_start_matches('/');
        let stem = tail
            .strip_suffix(".pyi")
            .or_else(|| tail.strip_suffix(".py"))?;
        let stem = stem.strip_suffix("/__init__").unwrap_or(stem);
        (stem != "__init__").then(|| stem.replace('/', "."))
    }

    /// `crate::a::b` path of a Rust file under its crate's `src/` (binaries excluded).
    fn rust_module(&self, rel: &str) -> Option<String> {
        let (dir, name) = self.crates.iter().find(|(d, _)| under(rel, d))?;
        let tail = rel.strip_prefix(dir.as_str())?.trim_start_matches('/');
        let tail = tail.strip_prefix("src/")?;
        if tail.starts_with("bin/") {
            return None;
        }
        if matches!(tail, "lib.rs" | "main.rs") {
            return Some(name.clone());
        }
        let stem = tail.strip_suffix(".rs")?;
        let stem = stem.strip_suffix("/mod").unwrap_or(stem);
        Some(format!("{name}::{}", stem.replace('/', "::")))
    }

    /// Import path of the package (directory) a Go file belongs to.
    fn go_package(&self, rel: &str) -> Option<String> {
        let (dir, module) = self.go_mods.iter().find(|(d, _)| under(rel, d))?;
        let file_dir = rel.rsplit_once('/').map_or("", |(d, _)| d);
        let tail = file_dir.strip_prefix(dir.as_str())?.trim_start_matches('/');
        Some(if tail.is_empty() {
            module.clone()
        } else {
            format!("{module}/{tail}")
        })
    }

    fn ts_specifiers(&self, rel: &str) -> Vec<String> {
        let Some(pkg) = self.ts_pkgs.iter().find(|p| under(rel, &p.dir)) else {
            return Vec::new();
        };
        let tail = rel.strip_prefix(pkg.dir.as_str()).unwrap_or(rel);
        let stem = strip_ts_ext(tail.trim_start_matches('/'));
        let mut out = vec![format!("{}/{}", pkg.name, stem)];
        if let Some(dir) = stem.strip_suffix("/index") {
            out.push(format!("{}/{}", pkg.name, dir));
        }
        if pkg.entry.as_deref() == Some(rel) {
            out.push(pkg.name.clone());
        }
        out
    }
}

/// True when `rel` lies inside repo-relative directory `dir` (`""` contains everything).
fn under(rel: &str, dir: &str) -> bool {
    dir.is_empty() || rel.strip_prefix(dir).is_some_and(|t| t.starts_with('/'))
}

pub fn strip_ts_ext(p: &str) -> &str {
    TS_EXTS.iter().find_map(|e| p.strip_suffix(e)).unwrap_or(p)
}

fn join(dir: &str, rel: &str) -> String {
    let rel = rel.trim_start_matches("./").trim_end_matches('/');
    match (dir.is_empty(), rel.is_empty() || rel == ".") {
        (_, true) => dir.to_string(),
        (true, false) => rel.to_string(),
        (false, false) => format!("{dir}/{rel}"),
    }
}

/// Source roots declared by a `pyproject.toml` in repo-relative `dir`.
fn python_roots(root: &Path, dir: &str, text: &str) -> Vec<String> {
    let mut out = vec![dir.to_string()];
    let Ok(v) = text.parse::<toml::Table>() else {
        return out;
    };
    let tool = v.get("tool");
    let strs = |x: Option<&toml::Value>| -> Vec<String> {
        x.and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let find = tool
        .and_then(|t| t.get("setuptools"))
        .and_then(|s| s.get("packages"))
        .and_then(|p| p.get("find"));
    out.extend(
        strs(find.and_then(|f| f.get("where")))
            .iter()
            .map(|w| join(dir, w)),
    );
    if let Some(pkgs) = tool
        .and_then(|t| t.get("poetry"))
        .and_then(|p| p.get("packages"))
        .and_then(|p| p.as_array())
    {
        for p in pkgs {
            if let Some(from) = p.get("from").and_then(|f| f.as_str()) {
                out.push(join(dir, from));
            }
        }
    }
    let hatch = tool
        .and_then(|t| t.get("hatch"))
        .and_then(|h| h.get("build"))
        .and_then(|b| b.get("targets"))
        .and_then(|t| t.get("wheel"))
        .and_then(|w| w.get("packages"));
    for p in strs(hatch) {
        if let Some((parent, _)) = p.trim_end_matches('/').rsplit_once('/') {
            out.push(join(dir, parent));
        }
    }
    let src = join(dir, "src");
    if root.join(&src).is_dir() {
        out.push(src);
    }
    out
}

/// Path mappings from a tsconfig/jsconfig, following local `extends` chains.
fn ts_config(root: &Path, dir: &str, text: &str) -> Option<TsConfig> {
    let mut cfg = TsConfig {
        dir: dir.to_string(),
        base: None,
        paths: Vec::new(),
    };
    let (mut cur_dir, mut cur_text) = (dir.to_string(), text.to_string());
    for _ in 0..5 {
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc(&cur_text)).ok()?;
        let opts = v.get("compilerOptions");
        let base_url = opts
            .and_then(|o| o.get("baseUrl"))
            .and_then(|b| b.as_str())
            .map(|b| join(&cur_dir, b));
        if cfg.base.is_none() {
            cfg.base = base_url.clone();
        }
        if cfg.paths.is_empty() {
            if let Some(p) = opts
                .and_then(|o| o.get("paths"))
                .and_then(|p| p.as_object())
            {
                // Without baseUrl, `paths` are relative to the config that declares them.
                let anchor = base_url.unwrap_or_else(|| cur_dir.clone());
                if cfg.base.is_none() {
                    cfg.base = Some(anchor);
                }
                cfg.paths = p
                    .iter()
                    .map(|(k, ts)| {
                        let ts = ts
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|t| t.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        (k.clone(), ts)
                    })
                    .collect();
            }
        }
        let Some(ext) = v.get("extends").and_then(|e| e.as_str()) else {
            break;
        };
        if !ext.starts_with('.') {
            break; // package-provided base config
        }
        let path = join(&cur_dir, ext);
        let path = if path.ends_with(".json") {
            path
        } else {
            format!("{path}.json")
        };
        cur_text = std::fs::read_to_string(root.join(&path)).ok()?;
        cur_dir = path
            .rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_string());
    }
    (cfg.base.is_some() || !cfg.paths.is_empty()).then_some(cfg)
}

/// Strip `//` and `/* */` comments and trailing commas from JSONC, leaving strings intact.
fn strip_jsonc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut prev = ' ';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            (',', _) => {
                let rest: String = chars.clone().collect();
                let next = rest.trim_start().chars().next();
                if !matches!(next, Some('}') | Some(']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// A `package.json` with a name; the entry is mapped back to a source file when possible.
fn ts_package(root: &Path, dir: &str, text: &str) -> Option<TsPackage> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let name = v.get("name")?.as_str()?.to_string();
    let exports_dot = v.get("exports").and_then(|e| match e {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(o) => o.get(".").and_then(|d| match d {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(c) => ["types", "import", "default", "require"]
                .iter()
                .find_map(|k| c.get(*k).and_then(|x| x.as_str()).map(str::to_string)),
            _ => None,
        }),
        _ => None,
    });
    let declared = ["types", "typings", "module", "main"]
        .iter()
        .filter_map(|k| v.get(*k).and_then(|x| x.as_str()).map(str::to_string))
        .chain(exports_dot);
    let mut candidates: Vec<String> = Vec::new();
    for d in declared {
        let stem = strip_ts_ext(d.trim_start_matches("./")).to_string();
        candidates.push(stem.clone());
        // Built output (`dist/index.js`) usually mirrors `src/index.ts`.
        for out_dir in ["dist/", "lib/", "build/", "out/"] {
            if let Some(rest) = stem.strip_prefix(out_dir) {
                candidates.push(format!("src/{rest}"));
            }
        }
    }
    candidates.extend(["src/index".to_string(), "index".to_string()]);
    let entry = candidates.iter().find_map(|stem| {
        [".ts", ".tsx", ".js", ".jsx", ".mjs"]
            .iter()
            .map(|e| join(dir, &format!("{stem}{e}")))
            .find(|p| root.join(p).is_file())
    });
    Some(TsPackage {
        dir: dir.to_string(),
        name,
        entry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_comments_and_trailing_commas() {
        let src = "{\n // c\n \"a\": \"x//y\", /* b */ \"p\": [1, 2,],\n}";
        let v: serde_json::Value = serde_json::from_str(&strip_jsonc(src)).unwrap();
        assert_eq!(v["a"], "x//y");
        assert_eq!(v["p"], serde_json::json!([1, 2]));
    }

    #[test]
    fn pyproject_source_roots() {
        let text =
            "[project]\nname = \"x\"\n\n[tool.setuptools.packages.find]\nwhere = [\"lib\"]\n";
        let roots = python_roots(Path::new("/nonexistent"), "svc", text);
        assert_eq!(roots, ["svc", "svc/lib"]);
    }

    #[test]
    fn fixture_module_names() {
        let common = RepoPackages::discover(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/org/pkg-common"
        )));
        assert_eq!(
            common.module_names("src/acme/common/http.py", &Language::Python),
            vec!["acme.common.http"]
        );
        assert_eq!(
            common.module_names("src/acme/common/__init__.py", &Language::Python),
            vec!["acme.common"]
        );

        let svc = RepoPackages::discover(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/org/service-a"
        )));
        assert_eq!(
            svc.module_names("service_a/users.py", &Language::Python),
            vec!["service_a.users"]
        );

        let kit = RepoPackages::discover(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/org/ui-kit"
        )));
        assert_eq!(
            kit.module_names("src/index.ts", &Language::TypeScript),
            vec!["@acme/ui-kit/src/index", "@acme/ui-kit/src", "@acme/ui-kit"]
        );
        assert_eq!(
            kit.module_names("src/button.ts", &Language::TypeScript),
            vec!["@acme/ui-kit/src/button"]
        );
    }
}
