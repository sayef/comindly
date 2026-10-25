//! Compiled constraint checks: `forbid [<edge-kind>] from <sel> to <sel>` and `forbid dep <pkg> [<ver>]`,
//! where a selector is `<glob over repo/path>[::<glob over name>]`.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobMatcher};
use serde::Serialize;

use crate::ledger::{Entry, Kind, Status, Trust};
use crate::manifests::PackageRow;
use crate::model::{Edge, EdgeKind, Symbol};

#[derive(Debug, Clone)]
pub struct Selector {
    path: GlobMatcher,
    name: Option<GlobMatcher>,
}

impl Selector {
    fn parse(s: &str) -> Result<Selector> {
        let (path, name) = match s.split_once("::") {
            Some((p, n)) => (p, Some(glob(n)?)),
            None => (s, None),
        };
        Ok(Selector {
            path: glob(path)?,
            name,
        })
    }

    fn matches(&self, s: &Symbol) -> bool {
        self.path.is_match(format!("{}/{}", s.repo.0, s.file_path))
            && self.name.as_ref().is_none_or(|n| n.is_match(&s.name))
    }
}

#[derive(Debug, Clone)]
pub enum Rule {
    Edge {
        kind: Option<EdgeKind>,
        from: Selector,
        to: Selector,
    },
    Dep {
        name: GlobMatcher,
        version: Option<GlobMatcher>,
    },
}

/// One place where the code breaks a rule.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub repo: String,
    pub path: String,
    pub line: u32,
    pub what: String,
}

/// A checked constraint and what it found.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub record: String,
    pub title: String,
    pub rule: String,
    pub severity: String,
    pub violations: Vec<Violation>,
}

fn glob(s: &str) -> Result<GlobMatcher> {
    Ok(Glob::new(s)
        .with_context(|| format!("bad glob `{s}`"))?
        .compile_matcher())
}

fn edge_kind(s: &str) -> Option<EdgeKind> {
    use EdgeKind::*;
    [
        Contains,
        Imports,
        Calls,
        Instantiates,
        Inherits,
        Implements,
        References,
        Defines,
        Uses,
        ParticipatesIn,
        Serves,
        Consumes,
        Publishes,
        Subscribes,
        Reads,
        Writes,
    ]
    .into_iter()
    .find(|k| k.as_str() == s)
}

impl Rule {
    pub fn parse(text: &str) -> Result<Rule> {
        let w: Vec<&str> = text.split_whitespace().collect();
        match w.as_slice() {
            ["forbid", "dep", name] => Ok(Rule::Dep {
                name: glob(name)?,
                version: None,
            }),
            ["forbid", "dep", name, v] => Ok(Rule::Dep {
                name: glob(name)?,
                version: Some(glob(v.trim_start_matches(['=', '^', '~']))?),
            }),
            ["forbid", "from", a, "to", b] => Ok(Rule::Edge {
                kind: None,
                from: Selector::parse(a)?,
                to: Selector::parse(b)?,
            }),
            ["forbid", k, "from", a, "to", b] => Ok(Rule::Edge {
                kind: Some(edge_kind(k).with_context(|| format!("unknown edge kind `{k}`"))?),
                from: Selector::parse(a)?,
                to: Selector::parse(b)?,
            }),
            _ => bail!(
                "expected `forbid [<edge-kind>] from <glob>[::<name>] to <glob>[::<name>]` \
                 or `forbid dep <package> [<version>]`"
            ),
        }
    }

    pub fn violations(
        &self,
        by_id: &HashMap<String, &Symbol>,
        edges: &[Edge],
        packages: &[PackageRow],
    ) -> Vec<Violation> {
        let mut out = Vec::new();
        match self {
            Rule::Edge { kind, from, to } => {
                for e in edges {
                    let wanted = match kind {
                        Some(k) => &e.kind == k,
                        None => e.kind != EdgeKind::Contains,
                    };
                    if !wanted {
                        continue;
                    }
                    let (Some(s), Some(d)) =
                        (by_id.get(&e.src.render()), by_id.get(&e.dst.render()))
                    else {
                        continue;
                    };
                    if from.matches(s) && to.matches(d) {
                        out.push(Violation {
                            repo: s.repo.0.clone(),
                            path: s.file_path.clone(),
                            line: s.range.start.line,
                            what: if d.file_path.is_empty() {
                                format!("`{}` {} `{}`", s.name, e.kind, d.name)
                            } else {
                                format!(
                                    "`{}` {} `{}` ({}/{})",
                                    s.name, e.kind, d.name, d.repo.0, d.file_path
                                )
                            },
                        });
                    }
                }
            }
            Rule::Dep { name, version } => {
                for p in packages
                    .iter()
                    .filter(|p| !p.publishes && name.is_match(&p.name))
                {
                    let hit = match version {
                        None => true,
                        Some(v) => [&p.locked, &p.version].into_iter().flatten().any(|x| {
                            v.is_match(x.trim_start_matches(['=', '^', '~', '>', '<', ' ']))
                        }),
                    };
                    if hit {
                        out.push(Violation {
                            repo: p.repo.clone(),
                            path: p.manifest.clone(),
                            line: 0,
                            what: format!(
                                "depends on {} {}",
                                p.name,
                                p.locked.as_deref().or(p.version.as_deref()).unwrap_or("*")
                            ),
                        });
                    }
                }
            }
        }
        out.sort_by(|a, b| (&a.repo, &a.path, a.line).cmp(&(&b.repo, &b.path, b.line)));
        out.dedup_by(|a, b| a.repo == b.repo && a.path == b.path && a.what == b.what);
        out
    }
}

/// Live attempts anchored to a dependency version (`dep=<pkg> <version>`) whose condition no
/// longer holds: the package is still used, but no manifest matches that version any more.
pub fn revivals<'e>(entries: &'e [Entry], packages: &[PackageRow]) -> Vec<(&'e Entry, String)> {
    let mut out = Vec::new();
    for e in entries {
        if e.record.kind != Kind::Attempt
            || matches!(e.status, Status::Retracted | Status::Superseded)
        {
            continue;
        }
        let Some(dep) = e.record.fields.get("dep").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(Rule::Dep {
            name,
            version: Some(v),
        }) = Rule::parse(&format!("forbid dep {dep}"))
        else {
            continue;
        };
        let pinned = Rule::Dep {
            name: name.clone(),
            version: Some(v),
        };
        let none = HashMap::new();
        let used = Rule::Dep {
            name,
            version: None,
        }
        .violations(&none, &[], packages);
        if !used.is_empty() && pinned.violations(&none, &[], packages).is_empty() {
            let now: std::collections::BTreeSet<&str> = used
                .iter()
                .filter_map(|u| u.what.rsplit(' ').next())
                .collect();
            out.push((
                e,
                format!(
                    "failed under `{dep}`; now on {}",
                    now.into_iter().collect::<Vec<_>>().join(", ")
                ),
            ));
        }
    }
    out
}

/// The rule text of a record, if it is a live constraint with a check.
pub fn rule_of(e: &Entry) -> Option<&str> {
    if e.record.kind != Kind::Constraint
        || matches!(e.status, Status::Retracted | Status::Superseded)
    {
        return None;
    }
    e.record.fields.get("check")?.as_str()
}

/// Evaluate every live constraint's check; unparsable rules are reported as one violation.
pub fn run(
    entries: &[Entry],
    symbols: &[Symbol],
    edges: &[Edge],
    packages: &[PackageRow],
) -> Vec<Finding> {
    let by_id: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.render(), s)).collect();
    entries
        .iter()
        .filter_map(|e| {
            let rule = rule_of(e)?;
            let violations = match Rule::parse(rule) {
                Ok(r) => r.violations(&by_id, edges, packages),
                Err(err) => vec![Violation {
                    repo: String::new(),
                    path: String::new(),
                    line: 0,
                    what: format!("rule does not parse: {err:#}"),
                }],
            };
            Some(Finding {
                record: e.record.id.clone(),
                title: e.record.title.clone(),
                rule: rule.to_string(),
                severity: e
                    .record
                    .fields
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("must")
                    .to_string(),
                violations,
            })
        })
        .collect()
}

/// Constraints whose check passes on the current code are enforced (T4).
pub fn promote(entries: &mut [Entry], findings: &[Finding]) {
    for e in entries.iter_mut() {
        if findings
            .iter()
            .any(|f| f.record == e.record.id && f.violations.is_empty())
        {
            e.trust = Trust::T4;
        }
    }
}

/// Markdown summary; `only` limits violations to these repo-relative files (`repo/path`).
pub fn markdown(findings: &[Finding], only: Option<&[String]>) -> String {
    let mut o = String::new();
    for f in findings {
        let shown: Vec<&Violation> = f
            .violations
            .iter()
            .filter(|v| only.is_none_or(|o| o.contains(&format!("{}/{}", v.repo, v.path))))
            .collect();
        let state = if f.violations.is_empty() {
            "passes".to_string()
        } else {
            format!("{} violation(s)", f.violations.len())
        };
        if only.is_some() && shown.is_empty() {
            continue;
        }
        o.push_str(&format!(
            "- [{} · {state}] {} — `{}` · id `{}`\n",
            f.severity,
            f.title,
            f.rule,
            &f.record[..f.record.len().min(13)]
        ));
        for v in shown.iter().take(20) {
            let at = if v.line > 0 {
                format!(":{}", v.line)
            } else {
                String::new()
            };
            o.push_str(&format!("  - {}/{}{at} {}\n", v.repo, v.path, v.what));
        }
        if shown.len() > 20 {
            o.push_str(&format!("  - … {} more\n", shown.len() - 20));
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rules() {
        assert!(Rule::parse("forbid calls from svc-a/handlers/** to */db/*.py::raw_*").is_ok());
        assert!(Rule::parse("forbid from a/** to b/**").is_ok());
        assert!(Rule::parse("forbid dep lance =1.0.1").is_ok());
        assert!(Rule::parse("forbid frobs from a to b").is_err());
        assert!(Rule::parse("allow everything").is_err());
    }

    #[test]
    fn edge_rule_matches_path_and_name() {
        use crate::model::{GlobalSymbolId, Language, Position, Range, RepoId, SymbolKind};
        let at = Position { line: 1, column: 0 };
        let sym = |repo: &str, path: &str, name: &str| Symbol {
            id: GlobalSymbolId {
                scheme: "comindly-treesitter".into(),
                package_manager: ".".into(),
                package: repo.into(),
                version: ".".into(),
                descriptor: format!("{path}/{name}()."),
            },
            name: name.into(),
            kind: SymbolKind::Function,
            language: Language::Python,
            repo: RepoId(repo.into()),
            file_path: path.into(),
            range: Range { start: at, end: at },
            signature: None,
            docstring: None,
            fingerprint: None,
        };
        let a = sym("svc", "app/handlers/user.py", "get_user");
        let b = sym("svc", "app/db/raw.py", "raw_query");
        let edge = Edge {
            src: a.id.clone(),
            dst: b.id.clone(),
            kind: EdgeKind::Calls,
            confidence: 1.0,
            cross_repo: false,
        };
        let by_id: HashMap<String, &Symbol> =
            [&a, &b].into_iter().map(|s| (s.id.render(), s)).collect();
        let hit = |r: &str| {
            Rule::parse(r)
                .unwrap()
                .violations(&by_id, std::slice::from_ref(&edge), &[])
                .len()
        };
        assert_eq!(hit("forbid calls from svc/app/handlers/** to **::raw_*"), 1);
        assert_eq!(hit("forbid from svc/app/handlers/** to svc/app/db/*"), 1);
        assert_eq!(hit("forbid imports from svc/** to svc/**"), 0);
        assert_eq!(hit("forbid calls from svc/app/db/** to **"), 0);
    }

    #[test]
    fn revival_when_pinned_version_is_gone() {
        use crate::ledger::{Anchor, AuthorKind, Provenance, Record};
        let row = |l: &str| PackageRow {
            repo: "r".into(),
            ecosystem: "cargo".into(),
            name: "lance".into(),
            publishes: false,
            version: Some(l.into()),
            locked: Some(l.into()),
            manifest: "Cargo.toml".into(),
        };
        let mut rec = Record::new(
            Kind::Attempt,
            "lance fragment reuse",
            Vec::<Anchor>::new(),
            Provenance {
                author: AuthorKind::Human,
                actor: "a".into(),
                source: "t".into(),
                agent: None,
                session: None,
                name: None,
            },
        );
        rec.fields.insert("dep".into(), "lance =1.0.1".into());
        let e = vec![Entry {
            record: rec,
            status: Status::Active,
            trust: Trust::T3,
            current: Vec::new(),
            related: Vec::new(),
            verified: None,
            by: String::new(),
            history: Vec::new(),
            retired: None,
        }];
        assert!(revivals(&e, &[row("1.0.1")]).is_empty());
        assert_eq!(revivals(&e, &[row("1.3.0")]).len(), 1);
        assert!(revivals(&e, &[]).is_empty());
    }

    #[test]
    fn dep_rule_matches_locked_or_declared() {
        let row = |v: &str, l: Option<&str>| PackageRow {
            repo: "r".into(),
            ecosystem: "cargo".into(),
            name: "lance".into(),
            publishes: false,
            version: Some(v.into()),
            locked: l.map(Into::into),
            manifest: "Cargo.toml".into(),
        };
        let r = Rule::parse("forbid dep lance =1.0.1").unwrap();
        let none = HashMap::new();
        assert_eq!(r.violations(&none, &[], &[row("=1.0.1", None)]).len(), 1);
        assert_eq!(
            r.violations(&none, &[], &[row("1.0", Some("1.0.1"))]).len(),
            1
        );
        assert!(r
            .violations(&none, &[], &[row("1.3", Some("1.3.2"))])
            .is_empty());
    }
}
