//! Recipes from history: when commits add code in a directory, which other directories change
//! with it ("adding a route also touches models/ and tests/api/"), with an exemplar commit.

use std::collections::{BTreeMap, BTreeSet};

use crate::conventions::Convention;
use crate::git::CommitFiles;

const SOURCE_EXTS: &[&str] = &[
    ".py", ".ts", ".tsx", ".js", ".jsx", ".go", ".java", ".kt", ".rs",
];
/// Commits touching more files than this are refactors/merges, not recipes.
const MAX_FILES: usize = 40;
const MIN_COMMITS: usize = 4;
/// Files that change with everything (versions, locks, changelogs) and say nothing about a recipe.
const NOISE: &[&str] = &[
    "uv.lock",
    "poetry.lock",
    "Cargo.lock",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "go.sum",
    "pyproject.toml",
    "package.json",
    "Cargo.toml",
    "CHANGELOG.md",
    "README.md",
    ".gitlab-ci.yml",
    "VERSION",
];

fn is_noise(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    NOISE.contains(&name) || name.starts_with("CHANGELOG")
}

pub fn mine(repo: &str, history: &[CommitFiles]) -> Vec<Convention> {
    // anchor dir → commits adding source there → co-changed dirs per commit
    let mut by_anchor: BTreeMap<String, Vec<(&CommitFiles, BTreeSet<String>)>> = BTreeMap::new();
    for c in history
        .iter()
        .filter(|c| c.changed.len() >= 2 && c.changed.len() <= MAX_FILES)
    {
        let anchors: BTreeSet<String> = c
            .added
            .iter()
            .filter(|p| SOURCE_EXTS.iter().any(|e| p.ends_with(e)))
            .map(|p| dir_of(p))
            .collect();
        for a in anchors {
            let co: BTreeSet<String> = c
                .changed
                .iter()
                .filter(|p| !is_noise(p))
                .map(|p| dir_of(p))
                .filter(|d| d != &a)
                .collect();
            if !co.is_empty() {
                by_anchor.entry(a).or_default().push((c, co));
            }
        }
    }
    let mut out = Vec::new();
    for (anchor, commits) in by_anchor {
        if commits.len() < MIN_COMMITS {
            continue;
        }
        let mut freq: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, co) in &commits {
            for d in co {
                *freq.entry(d.as_str()).or_default() += 1;
            }
        }
        let mut with: Vec<(&str, usize)> = freq
            .into_iter()
            .filter(|(_, n)| n * 2 >= commits.len() && *n >= 3)
            .collect();
        if with.is_empty() {
            continue;
        }
        with.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        with.truncate(4);
        let dirs: BTreeSet<&str> = with.iter().map(|(d, _)| *d).collect();
        // Most recent commit covering the most of the recipe.
        let (example, _) = commits
            .iter()
            .max_by_key(|(_, co)| co.iter().filter(|d| dirs.contains(d.as_str())).count())
            .expect("non-empty");
        let parts: Vec<String> = with
            .iter()
            .map(|(d, n)| format!("`{d}/` ({}%)", n * 100 / commits.len()))
            .collect();
        let generic = format!(
            "Adding code in `{anchor}/` usually comes with changes in {}",
            parts.join(", ")
        );
        out.push(Convention {
            origin: crate::conventions::Origin::Measured,
            repo: repo.into(),
            dimension: "recipes".into(),
            key: format!("recipes:{repo}:{anchor}"),
            rule: format!("{generic} — {} commits", commits.len()),
            generic,
            support: with[0].1,
            total: commits.len(),
            exemplars: vec![format!(
                "{} {}",
                &example.sha[..example.sha.len().min(8)],
                example.subject
            )],
            counterexamples: Vec::new(),
        });
    }
    out
}

fn dir_of(path: &str) -> String {
    path.rsplit_once('/').map_or(".", |(d, _)| d).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(sha: &str, added: &[&str], changed: &[&str]) -> CommitFiles {
        CommitFiles {
            sha: sha.into(),
            subject: format!("commit {sha}"),
            added: added.iter().map(|s| s.to_string()).collect(),
            changed: changed.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn route_recipe_from_history() {
        let h: Vec<CommitFiles> = (0..5)
            .map(|i| {
                let route = format!("app/api/routers/r{i}.py");
                c(
                    &format!("sha{i}xxxxx"),
                    &[&route],
                    &[
                        &route,
                        "app/api/models.py",
                        &format!("tests/api/test_r{i}.py"),
                    ],
                )
            })
            .collect();
        let r = mine("svc", &h);
        assert_eq!(r.len(), 1, "{r:?}");
        assert!(r[0].rule.contains("`app/api/routers/`"));
        assert!(r[0].rule.contains("`app/api/` (100%)"));
        assert!(r[0].rule.contains("`tests/api/` (100%)"));
    }
}
