//! Values from a repo's YAML config files (`config*.yml`, `settings*.yml`, `application*.yml`),
//! kept as facts so settings paths the code cannot resolve (`config.task_queue.queue`) still
//! name their queue, bucket or URL.

use std::path::Path;

use ignore::WalkBuilder;
use yaml_rust2::{Yaml, YamlLoader};

use crate::facts::{FileFacts, ValueExpr, ValueFact};
use crate::model::Language;

/// Scope prefix of config values; the full scope is `config:<repo>`.
pub const SCOPE: &str = "config:";

pub fn is_config(rel: &str) -> bool {
    let p = rel.to_ascii_lowercase();
    let file = p.rsplit('/').next().unwrap_or(&p);
    let stem_ok = ["config", "settings", "application"]
        .iter()
        .any(|s| file.starts_with(s));
    let in_skipped = p.split('/').any(|d| {
        matches!(
            d,
            "node_modules" | ".github" | "tests" | "test" | "fixtures" | ".venv" | "target"
        )
    });
    stem_ok
        && p.matches('/').count() < 6
        && (file.ends_with(".yml") || file.ends_with(".yaml"))
        && !in_skipped
}

/// Facts for every config file in a repo.
pub fn scan(root: &Path, repo: &str) -> Vec<FileFacts> {
    let mut rels: Vec<String> = WalkBuilder::new(root)
        .hidden(false)
        .max_depth(Some(6))
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|e| {
            e.path()
                .strip_prefix(root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .filter(|r| is_config(r))
        .collect();
    rels.sort();
    rels.iter().filter_map(|r| file(root, repo, r)).collect()
}

/// Facts for one config file: one value per scalar leaf, keyed by its dotted path.
pub fn file(root: &Path, repo: &str, rel: &str) -> Option<FileFacts> {
    let text = std::fs::read_to_string(root.join(rel)).ok()?;
    let docs = YamlLoader::load_from_str(&text).ok()?;
    let mut values = Vec::new();
    for d in &docs {
        leaves(d, &mut Vec::new(), &mut values);
    }
    let scope = format!("{SCOPE}{repo}");
    Some(FileFacts {
        repo: repo.to_string(),
        path: rel.to_string(),
        language: Language::Other("yaml".into()),
        file_id: format!("{SCOPE}{repo}:{rel}"),
        modules: Vec::new(),
        imports: Vec::new(),
        refs: Vec::new(),
        types: Vec::new(),
        scip: None,
        contracts: Vec::new(),
        values: values
            .into_iter()
            .map(|(name, v)| ValueFact {
                scope: scope.clone(),
                name,
                value: ValueExpr::Lit(v),
            })
            .collect(),
        params: Vec::new(),
        calls: Vec::new(),
    })
}

fn leaves(y: &Yaml, path: &mut Vec<String>, out: &mut Vec<(String, String)>) {
    match y {
        Yaml::Hash(h) => {
            for (k, v) in h {
                let Some(k) = scalar(k) else {
                    continue;
                };
                path.push(k);
                leaves(v, path, out);
                path.pop();
            }
        }
        other => {
            if let (Some(v), false) = (scalar(other).and_then(concrete), path.is_empty()) {
                out.push((path.join("."), v));
            }
        }
    }
}

fn scalar(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.clone()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

/// A literal value; `${VAR:default}` yields its default, other templates are skipped.
fn concrete(v: String) -> Option<String> {
    if let Some(inner) = v.strip_prefix("${").and_then(|r| r.strip_suffix('}')) {
        return inner
            .split_once(':')
            .map(|(_, d)| d.to_string())
            .filter(|d| !d.is_empty() && !d.contains("${"));
    }
    (!v.contains("${") && !v.contains("{{")).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_are_dotted_and_concrete() {
        let docs = YamlLoader::load_from_str(
            "task_queue:\n  queue: jobs-q\n  url: ${QUEUE_URL}\nport: 8080\nbucket: ${B:files}\n",
        )
        .unwrap();
        let mut out = Vec::new();
        leaves(&docs[0], &mut Vec::new(), &mut out);
        assert_eq!(
            out,
            vec![
                ("task_queue.queue".to_string(), "jobs-q".to_string()),
                ("port".to_string(), "8080".to_string()),
                ("bucket".to_string(), "files".to_string()),
            ]
        );
        assert!(is_config("app/config.local.yml"));
        assert!(is_config("src/main/resources/application-prod.yaml"));
        assert!(!is_config("app/tests/config.yml"));
        assert!(!is_config(".gitlab-ci.yml"));
    }
}
