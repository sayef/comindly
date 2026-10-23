//! Packages and versions from manifests and lockfiles: what each repo publishes and depends on,
//! so internal dependencies map to the repo that publishes them (and version skew shows).

use std::collections::HashMap;
use std::path::Path;

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRow {
    pub repo: String,
    /// `pypi` | `npm` | `go` | `cargo`.
    pub ecosystem: String,
    /// Normalized package name.
    pub name: String,
    /// `true`: this repo publishes the package; `false`: it depends on it.
    pub publishes: bool,
    /// Published version, or the declared requirement for a dependency.
    pub version: Option<String>,
    /// Version pinned by the lockfile next to the manifest.
    pub locked: Option<String>,
    /// Repo-relative manifest path.
    pub manifest: String,
}

/// An internal dependency: `consumer` uses `name`, which `publisher` publishes.
#[derive(Debug, Clone)]
pub struct InternalDep {
    pub ecosystem: String,
    pub name: String,
    pub publisher: String,
    pub published: Option<String>,
    pub consumer: String,
    pub requirement: Option<String>,
    pub locked: Option<String>,
}

impl InternalDep {
    /// The consumer pins a version other than the one the publisher's source declares.
    pub fn skewed(&self) -> bool {
        matches!((&self.locked, &self.published), (Some(l), Some(p)) if l != p)
    }
}

/// Read every manifest in a repo (respecting `.gitignore`, skipping vendored trees).
pub fn scan(root: &Path, repo: &str) -> Vec<PackageRow> {
    let mut rows = Vec::new();
    let files = WalkBuilder::new(root)
        .hidden(false)
        .max_depth(Some(5))
        .filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some("node_modules" | ".venv" | "venv" | "target" | ".git" | ".terraform")
            )
        })
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()));
    for e in files {
        let name = e.file_name().to_string_lossy().to_string();
        let Ok(rel) = e.path().strip_prefix(root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let dir = e.path().parent().unwrap_or(root);
        let read = || std::fs::read_to_string(e.path()).ok();
        let row =
            |ecosystem: &str, name: &str, publishes: bool, version: Option<String>| PackageRow {
                repo: repo.to_string(),
                ecosystem: ecosystem.to_string(),
                name: normalize(ecosystem, name),
                publishes,
                version,
                locked: None,
                manifest: rel.clone(),
            };
        match name.as_str() {
            "pyproject.toml" => {
                let Some(v) = read().and_then(|t| t.parse::<toml::Value>().ok()) else {
                    continue;
                };
                let locks = python_locks(dir);
                let project = v.get("project");
                let poetry = v.get("tool").and_then(|t| t.get("poetry"));
                if let Some(n) = project
                    .or(poetry)
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                {
                    let ver = project
                        .or(poetry)
                        .and_then(|p| p.get("version"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    rows.push(row("pypi", n, true, ver));
                }
                let deps = project
                    .and_then(|p| p.get("dependencies"))
                    .and_then(|d| d.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d.as_str())
                    .filter_map(pep508);
                let poetry_deps = poetry
                    .and_then(|p| p.get("dependencies"))
                    .and_then(|d| d.as_table())
                    .into_iter()
                    .flatten()
                    .filter(|(k, _)| k.as_str() != "python")
                    .map(|(k, v)| {
                        let spec = v
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| v.get("version")?.as_str().map(str::to_string));
                        (k.clone(), spec)
                    });
                for (n, spec) in deps.chain(poetry_deps) {
                    let mut r = row("pypi", &n, false, spec);
                    r.locked = locks.get(&r.name).cloned();
                    rows.push(r);
                }
            }
            _ if name.starts_with("requirements") && name.ends_with(".txt") => {
                for line in read().unwrap_or_default().lines() {
                    let line = line.split('#').next().unwrap_or_default().trim();
                    if line.is_empty() || line.starts_with('-') {
                        continue;
                    }
                    if let Some((n, spec)) = pep508(line) {
                        let mut r = row("pypi", &n, false, spec.clone());
                        r.locked = spec.and_then(|s| s.strip_prefix("==").map(str::to_string));
                        rows.push(r);
                    }
                }
            }
            "package.json" => {
                let Some(v) =
                    read().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                else {
                    continue;
                };
                let locks = npm_locks(dir);
                if let Some(n) = v.get("name").and_then(|n| n.as_str()) {
                    let ver = v
                        .get("version")
                        .and_then(|x| x.as_str())
                        .map(str::to_string);
                    rows.push(row("npm", n, true, ver));
                }
                for key in ["dependencies", "peerDependencies", "devDependencies"] {
                    for (n, spec) in v.get(key).and_then(|d| d.as_object()).into_iter().flatten() {
                        let mut r = row("npm", n, false, spec.as_str().map(str::to_string));
                        r.locked = locks.get(&r.name).cloned();
                        rows.push(r);
                    }
                }
            }
            "go.mod" => {
                let text = read().unwrap_or_default();
                let mut in_require = false;
                for line in text.lines() {
                    let l = line.split("//").next().unwrap_or_default().trim();
                    if let Some(m) = l.strip_prefix("module ") {
                        rows.push(row("go", m.trim(), true, None));
                    } else if l.starts_with("require (") {
                        in_require = true;
                    } else if in_require && l == ")" {
                        in_require = false;
                    } else if in_require || l.starts_with("require ") {
                        let l = l.trim_start_matches("require ").trim();
                        let mut parts = l.split_whitespace();
                        if let (Some(n), Some(ver)) = (parts.next(), parts.next()) {
                            let mut r = row("go", n, false, Some(ver.to_string()));
                            r.locked = Some(ver.to_string());
                            rows.push(r);
                        }
                    }
                }
            }
            "Cargo.toml" => {
                let Some(v) = read().and_then(|t| t.parse::<toml::Value>().ok()) else {
                    continue;
                };
                let locks = cargo_locks(dir);
                if let Some(p) = v.get("package") {
                    if let Some(n) = p.get("name").and_then(|n| n.as_str()) {
                        let ver = p
                            .get("version")
                            .and_then(|x| x.as_str())
                            .map(str::to_string);
                        rows.push(row("cargo", n, true, ver));
                    }
                }
                for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                    for (n, spec) in v.get(key).and_then(|d| d.as_table()).into_iter().flatten() {
                        let s = spec
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| spec.get("version")?.as_str().map(str::to_string));
                        let mut r = row("cargo", n, false, s);
                        r.locked = locks.get(&r.name).cloned();
                        rows.push(r);
                    }
                }
            }
            _ => {}
        }
    }
    rows
}

/// Internal dependencies: dependency rows whose package another repo in the corpus publishes.
pub fn internal(rows: &[PackageRow]) -> Vec<InternalDep> {
    let published: HashMap<(&str, &str), &PackageRow> = rows
        .iter()
        .filter(|r| r.publishes)
        .map(|r| ((r.ecosystem.as_str(), r.name.as_str()), r))
        .collect();
    let mut out: Vec<InternalDep> = rows
        .iter()
        .filter(|r| !r.publishes)
        .filter_map(|r| {
            let p = published.get(&(r.ecosystem.as_str(), r.name.as_str()))?;
            (p.repo != r.repo).then(|| InternalDep {
                ecosystem: r.ecosystem.clone(),
                name: r.name.clone(),
                publisher: p.repo.clone(),
                published: p.version.clone(),
                consumer: r.repo.clone(),
                requirement: r.version.clone(),
                locked: r.locked.clone(),
            })
        })
        .collect();
    out.sort_by(|a, b| (&a.name, &a.consumer).cmp(&(&b.name, &b.consumer)));
    out.dedup_by(|a, b| a.name == b.name && a.consumer == b.consumer);
    out
}

/// PEP 503 for PyPI (`Foo_Bar` → `foo-bar`); other ecosystems are case-sensitive as written.
fn normalize(ecosystem: &str, name: &str) -> String {
    if ecosystem == "pypi" {
        let lower = name.to_ascii_lowercase();
        let mut out = String::with_capacity(lower.len());
        for c in lower.chars() {
            let c = if matches!(c, '_' | '.') { '-' } else { c };
            if !(c == '-' && out.ends_with('-')) {
                out.push(c);
            }
        }
        out
    } else if ecosystem == "cargo" {
        name.replace('_', "-")
    } else {
        name.to_string()
    }
}

/// `acme[pgsql] >= 0.11.33 ; python_version > "3"` → (`acme`, `>= 0.11.33`).
fn pep508(spec: &str) -> Option<(String, Option<String>)> {
    let spec = spec.split(';').next()?.trim();
    let end = spec
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
        .unwrap_or(spec.len());
    let name = &spec[..end];
    if name.is_empty() {
        return None;
    }
    let rest = spec[end..].trim();
    let rest = match rest.strip_prefix('[') {
        Some(r) => r.split_once(']').map_or("", |(_, t)| t).trim(),
        None => rest,
    };
    Some((
        name.to_string(),
        (!rest.is_empty()).then(|| rest.replace(' ', "")),
    ))
}

/// Locked versions from `uv.lock` / `poetry.lock` next to a pyproject.
fn python_locks(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for f in ["uv.lock", "poetry.lock"] {
        let Some(v) = std::fs::read_to_string(dir.join(f))
            .ok()
            .and_then(|t| t.parse::<toml::Value>().ok())
        else {
            continue;
        };
        for p in v
            .get("package")
            .and_then(|p| p.as_array())
            .into_iter()
            .flatten()
        {
            if let (Some(n), Some(ver)) = (
                p.get("name").and_then(|n| n.as_str()),
                p.get("version").and_then(|n| n.as_str()),
            ) {
                out.insert(normalize("pypi", n), ver.to_string());
            }
        }
    }
    out
}

fn cargo_locks(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(v) = std::fs::read_to_string(dir.join("Cargo.lock"))
        .ok()
        .and_then(|t| t.parse::<toml::Value>().ok())
    else {
        return out;
    };
    for p in v
        .get("package")
        .and_then(|p| p.as_array())
        .into_iter()
        .flatten()
    {
        if let (Some(n), Some(ver)) = (
            p.get("name").and_then(|n| n.as_str()),
            p.get("version").and_then(|n| n.as_str()),
        ) {
            out.insert(normalize("cargo", n), ver.to_string());
        }
    }
    out
}

/// Locked versions from `package-lock.json` (v2/v3) or a pnpm lockfile's package keys.
fn npm_locks(dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(v) = std::fs::read_to_string(dir.join("package-lock.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    {
        for (k, p) in v
            .get("packages")
            .and_then(|p| p.as_object())
            .into_iter()
            .flatten()
        {
            if let (Some(n), Some(ver)) = (
                k.rsplit("node_modules/").next().filter(|n| !n.is_empty()),
                p.get("version").and_then(|x| x.as_str()),
            ) {
                out.entry(n.to_string()).or_insert_with(|| ver.to_string());
            }
        }
    }
    if let Ok(text) = std::fs::read_to_string(dir.join("pnpm-lock.yaml")) {
        let mut in_packages = false;
        for line in text.lines() {
            if !line.starts_with(' ') {
                in_packages = line.starts_with("packages:");
                continue;
            }
            if !in_packages || line.starts_with("    ") {
                continue;
            }
            // `  '@scope/name@1.2.3':` or `  name@1.2.3(peer@x):`
            let key = line.trim().trim_end_matches(':').trim_matches('\'');
            let key = key.trim_start_matches('/');
            if let Some(at) = key[1..].find('@').map(|i| i + 1) {
                let ver = key[at + 1..].split('(').next().unwrap_or_default();
                out.entry(key[..at].to_string())
                    .or_insert_with(|| ver.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pep508_specs() {
        assert_eq!(
            pep508("acme[fastapi,llm, pgsql] >= 0.12.5"),
            Some(("acme".into(), Some(">=0.12.5".into())))
        );
        assert_eq!(pep508("httpx"), Some(("httpx".into(), None)));
        assert_eq!(
            pep508("acme-common==1.4.0 ; python_version >= '3.11'"),
            Some(("acme-common".into(), Some("==1.4.0".into())))
        );
        assert_eq!(normalize("pypi", "Acme_Common"), "acme-common");
    }

    #[test]
    fn fixture_internal_dependencies() {
        let mut rows = Vec::new();
        for repo in [
            "pkg-common",
            "service-a",
            "ui-kit",
            "web-app",
            "rs-lib",
            "rs-app",
        ] {
            rows.extend(scan(
                &Path::new(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../tests/fixtures/org"
                ))
                .join(repo),
                repo,
            ));
        }
        let deps = internal(&rows);
        let pairs: Vec<(&str, &str, &str)> = deps
            .iter()
            .map(|d| (d.name.as_str(), d.publisher.as_str(), d.consumer.as_str()))
            .collect();
        assert!(
            pairs.contains(&("acme-common", "pkg-common", "service-a")),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&("@acme/ui-kit", "ui-kit", "web-app")),
            "{pairs:?}"
        );
        assert!(
            pairs.contains(&("acme-core", "rs-lib", "rs-app")),
            "{pairs:?}"
        );
        let common = deps.iter().find(|d| d.name == "acme-common").unwrap();
        assert_eq!(common.published.as_deref(), Some("1.4.0"));
        assert_eq!(common.requirement.as_deref(), Some("==1.4.0"));
    }
}
