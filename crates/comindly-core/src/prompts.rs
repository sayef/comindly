//! The LLM prompts indexing uses: built in (v0) or an activated version, stored with the index
//! (`<index>/_team`, table `prompts`), for what the next index writes; nothing is redone for them.

use std::collections::BTreeMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

const TABLE: &str = "prompts";
const MAX_LEN: usize = 8000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Task {
    Summary,
    Guide,
    Flow,
}

impl Task {
    pub const ALL: [Task; 3] = [Task::Summary, Task::Guide, Task::Flow];

    pub fn name(self) -> &'static str {
        match self {
            Task::Summary => "summary",
            Task::Guide => "guide",
            Task::Flow => "flow",
        }
    }

    pub fn parse(s: &str) -> Option<Task> {
        Task::ALL.into_iter().find(|t| t.name() == s)
    }

    /// The built-in instructions, version 0.
    pub fn builtin(self) -> &'static str {
        match self {
            Task::Summary => "You document code for a search index.",
            Task::Guide => {
                "You are a senior engineer documenting ONE section of a codebase's house-style \
                guide so an AI coding agent writes code INDISTINGUISHABLE from this repo's. You are \
                given REAL evidence (signatures, code excerpts, dependency facts, measured stats) for \
                this section only. Make the repo's tacit conventions explicit.\n\
                Hard rules:\n\
                - Cite a real FILE PATH (and symbol/signature) for every non-trivial claim; quote a \
                short real snippet as the example. Where useful, show a correct vs incorrect example.\n\
                - Prefer the repo's OWN wrapper/abstraction over a raw library when the evidence \
                shows one; name it exactly.\n\
                - NEVER output generic advice that would apply to any codebase ('use meaningful \
                names', 'write tests', 'handle errors gracefully'). Delete such lines.\n\
                - Strength tags: **MUST**/**NEVER** for patterns across ≥3 files or enforced by \
                config; 'prefer' for weaker signals. Give a one-line reason where non-obvious."
            }
            Task::Flow => {
                "You explain how a code flow works to a developer new to the codebase. The \
                narration is a 2-5 sentence walkthrough of what happens step by step, referencing \
                the actual symbol names."
            }
        }
    }

    /// What the reply holds; the same shape goes along as a JSON schema, and is never editable.
    pub fn format(self) -> &'static str {
        match self {
            Task::Summary => {
                "Reply as JSON: \"summary\" is one concise sentence describing what this does; \
                \"queries\" are 3 natural-language questions a developer might ask that this code \
                answers."
            }
            Task::Guide => {
                "Reply as JSON: \"rules\" holds this section's rules, one terse markdown rule per \
                item, with no heading, preamble or closing note; \"observed\" is false, with no \
                rules, when the evidence shows no consistent convention."
            }
            Task::Flow => {
                "You get an entry point and its call trace (each line: depth, relation, symbol, \
                location). Reply as JSON: \"narration\" is the walkthrough; \"queries\" are 3 \
                questions a developer might ask about this flow."
            }
        }
    }
}

impl Task {
    /// The shape of a reply, filled in, so a model without structured output does not echo the schema.
    fn example(self) -> &'static str {
        match self {
            Task::Summary => {
                r#"{"summary": "Opens a pooled database connection from the service settings.", "queries": ["How do I connect to the database?", "Where is the connection pool created?", "Which settings does the pool read?"]}"#
            }
            Task::Guide => {
                r#"{"observed": true, "rules": ["**MUST** name functions in `snake_case`, as in `src/db/pool.rs` `open_pool`."]}"#
            }
            Task::Flow => {
                r#"{"narration": "`handle_order` validates the order with `validate`, then stores it with `save`.", "queries": ["What happens when an order arrives?", "Where is an order validated?", "Where is an order saved?"]}"#
            }
        }
    }

    /// What is always added after the instructions: the format and its JSON schema, so a server
    /// that ignores structured output still gets the shape.
    pub fn fixed(self) -> String {
        format!(
            "{}\n\nReply with one JSON object that matches this schema (not the schema itself):\n{}\n\nFor example:\n{}",
            self.format(),
            crate::llm::reply_schema(self),
            self.example()
        )
    }
}

/// One saved version of a task's instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub task: Task,
    pub version: u32,
    pub text: String,
    /// Style guide only: what to cover in a section, by title, in place of the built-in guidance.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sections: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub author: String,
    pub created_at: String,
    /// The built-in prompt when this version was written, to tell when a release changed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub based_on: Option<String>,
}

impl Task {
    /// Changes whenever a release changes this task's built-in prompt.
    pub fn builtin_hash(self) -> String {
        let mut h = blake3::Hasher::new();
        h.update(self.builtin().as_bytes());
        if self == Task::Guide {
            for (title, guidance) in crate::styleguide::SECTIONS {
                h.update(b"\0");
                h.update(title.as_bytes());
                h.update(b"\0");
                h.update(guidance.as_bytes());
            }
        }
        h.finalize().to_hex().to_string()
    }
}

/// Every saved version and which one each task uses; a task with none active uses v0.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub versions: Vec<Version>,
    #[serde(default)]
    pub active: BTreeMap<Task, u32>,
}

impl Library {
    /// Save `text` as the task's next version, inactive until someone activates it.
    pub fn add(
        &mut self,
        task: Task,
        text: &str,
        sections: &BTreeMap<String, String>,
        note: Option<&str>,
        by: &str,
    ) -> Result<u32> {
        let text = text.trim();
        anyhow::ensure!(!text.is_empty(), "the prompt is empty");
        anyhow::ensure!(
            text.chars().count() <= MAX_LEN,
            "the prompt is longer than {MAX_LEN} characters"
        );
        let mut kept = BTreeMap::new();
        for (title, guidance) in sections {
            let builtin = crate::styleguide::SECTIONS
                .iter()
                .find(|(t, _)| t == title)
                .map(|(_, g)| *g);
            anyhow::ensure!(
                task == Task::Guide && builtin.is_some(),
                "no style guide section {title}"
            );
            let g = guidance.trim();
            anyhow::ensure!(
                g.chars().count() <= MAX_LEN,
                "the guidance for {title} is too long"
            );
            if !g.is_empty() && Some(g) != builtin {
                kept.insert(title.clone(), g.to_string());
            }
        }
        let version = self.of(task).map(|v| v.version).max().unwrap_or(0) + 1;
        self.versions.push(Version {
            task,
            version,
            text: text.to_string(),
            sections: kept,
            note: note
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            author: by.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            based_on: Some(task.builtin_hash()),
        });
        Ok(version)
    }

    pub fn of(&self, task: Task) -> impl Iterator<Item = &Version> {
        self.versions.iter().filter(move |v| v.task == task)
    }

    fn find(&self, task: Task, version: u32) -> Option<&Version> {
        self.of(task).find(|v| v.version == version)
    }

    /// Use `version` for `task` from the next index on, in place of whichever was active.
    pub fn activate(&mut self, task: Task, version: u32) -> Result<()> {
        anyhow::ensure!(
            self.find(task, version).is_some(),
            "{} has no version {version}",
            task.name()
        );
        self.active.insert(task, version);
        Ok(())
    }

    /// Go back to the built-in prompt.
    pub fn deactivate(&mut self, task: Task) {
        self.active.remove(&task);
    }

    /// Delete a version that is not in use.
    pub fn remove(&mut self, task: Task, version: u32) -> Result<()> {
        anyhow::ensure!(
            self.active.get(&task) != Some(&version),
            "deactivate version {version} before deleting it"
        );
        let before = self.versions.len();
        self.versions
            .retain(|v| !(v.task == task && v.version == version));
        anyhow::ensure!(
            self.versions.len() < before,
            "{} has no version {version}",
            task.name()
        );
        Ok(())
    }

    /// The active version of `task` when the built-in prompt changed since it was written.
    pub fn builtin_changed(&self, task: Task) -> Option<u32> {
        let n = *self.active.get(&task)?;
        let v = self.find(task, n)?;
        v.based_on
            .as_deref()
            .is_some_and(|b| b != task.builtin_hash())
            .then_some(n)
    }

    /// The prompts an index run uses.
    pub fn in_use(&self) -> Prompts {
        Prompts(
            self.active
                .iter()
                .filter_map(|(t, n)| {
                    let v = self.find(*t, *n)?;
                    Some((*t, (*n, v.text.clone(), v.sections.clone())))
                })
                .collect(),
        )
    }
}

type InUse = (u32, String, BTreeMap<String, String>);

/// The instructions each task runs with, fixed for one index run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prompts(BTreeMap<Task, InUse>);

impl Prompts {
    /// Only `task` changed, to unsaved `text` and section guidance, for trying it.
    pub fn draft(task: Task, text: &str, sections: BTreeMap<String, String>) -> Prompts {
        Prompts(BTreeMap::from([(
            task,
            (0, text.trim().to_string(), sections),
        )]))
    }

    pub fn version(&self, task: Task) -> u32 {
        self.0.get(&task).map_or(0, |(n, ..)| *n)
    }

    pub fn system(&self, task: Task) -> String {
        let text = self
            .0
            .get(&task)
            .map_or(task.builtin(), |(_, t, _)| t.as_str());
        format!("{text}\n\n{}", task.fixed())
    }

    /// What to cover in the style guide section `title`, when a version replaces the built-in.
    pub fn guidance(&self, title: &str) -> Option<&str> {
        let (_, _, s) = self.0.get(&Task::Guide)?;
        s.get(title).map(String::as_str)
    }

    /// e.g. `summary v2, guide built-in, flow built-in`.
    pub fn describe(&self) -> String {
        Task::ALL
            .iter()
            .map(|t| match self.version(*t) {
                0 => format!("{} built-in", t.name()),
                n => format!("{} v{n}", t.name()),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn uri(root: &str) -> String {
    format!("{}/_team", root.trim_end_matches('/'))
}

pub async fn load(root: &str) -> Result<crate::docstore::Stored<Library>> {
    crate::docstore::load(&uri(root), TABLE).await
}

/// Save `library` as `by` if nobody saved since `expected`.
pub async fn save_as(
    root: &str,
    library: &Library,
    expected: Option<u64>,
    by: &str,
) -> Result<u64> {
    crate::docstore::save(&uri(root), TABLE, library, expected, by)
        .await
        .map_err(|e| match format!("{e}").contains("changed meanwhile") {
            true => {
                anyhow::anyhow!("the prompts were changed by someone else; reload and try again")
            }
            false => e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_activated_version_is_used() {
        let mut l = Library::default();
        let v1 = l
            .add(
                Task::Summary,
                "Summarise for search.",
                &BTreeMap::new(),
                None,
                "a",
            )
            .unwrap();
        assert_eq!(
            l.in_use().version(Task::Summary),
            0,
            "a saved draft is not used"
        );
        l.activate(Task::Summary, v1).unwrap();
        let p = l.in_use();
        assert_eq!(p.version(Task::Summary), 1);
        assert!(p.system(Task::Summary).starts_with("Summarise for search."));
        assert!(
            p.system(Task::Summary).contains("\"queries\""),
            "the format stays"
        );
        assert_eq!(p.version(Task::Flow), 0);
        assert!(l.remove(Task::Summary, v1).is_err(), "the active one stays");
        l.deactivate(Task::Summary);
        assert_eq!(l.in_use(), Prompts::default());
        l.remove(Task::Summary, v1).unwrap();
        assert!(l.activate(Task::Summary, v1).is_err());
    }

    #[test]
    fn versions_count_per_task() {
        let mut l = Library::default();
        assert_eq!(
            l.add(Task::Guide, "a", &BTreeMap::new(), None, "x")
                .unwrap(),
            1
        );
        assert_eq!(
            l.add(Task::Guide, "b", &BTreeMap::new(), Some(" "), "x")
                .unwrap(),
            2
        );
        assert_eq!(
            l.add(Task::Flow, "c", &BTreeMap::new(), None, "x").unwrap(),
            1
        );
        assert_eq!(l.versions[1].note, None);
        assert!(l
            .add(Task::Flow, "  ", &BTreeMap::new(), None, "x")
            .is_err());
    }

    #[test]
    fn guide_versions_replace_section_guidance() {
        let mut l = Library::default();
        let (title, builtin) = crate::styleguide::SECTIONS[2];
        let sections = BTreeMap::from([
            (title.to_string(), "Only casing.".to_string()),
            (
                "Testing".to_string(),
                crate::styleguide::SECTIONS[8].1.to_string(),
            ),
        ]);
        let n = l
            .add(Task::Guide, Task::Guide.builtin(), &sections, None, "a")
            .unwrap();
        assert_eq!(
            l.versions[0].sections.len(),
            1,
            "unchanged guidance is not stored"
        );
        let bad = BTreeMap::from([("Nope".to_string(), "x".to_string())]);
        assert!(l.add(Task::Guide, "x", &bad, None, "a").is_err());
        assert!(l.add(Task::Summary, "x", &sections, None, "a").is_err());
        l.activate(Task::Guide, n).unwrap();
        let p = l.in_use();
        assert_eq!(p.guidance(title), Some("Only casing."));
        assert_ne!(p.guidance(title), Some(builtin));
        assert_eq!(l.builtin_changed(Task::Guide), None);
        l.versions[0].based_on = Some("an older release".into());
        assert_eq!(l.builtin_changed(Task::Guide), Some(n));
    }

    #[tokio::test]
    async fn saved_with_the_index() {
        let dir = std::env::temp_dir().join(format!("comindly-prompts-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        let mut l = Library::default();
        let n = l
            .add(
                Task::Flow,
                "Walk through it.",
                &BTreeMap::new(),
                Some("shorter"),
                "a",
            )
            .unwrap();
        l.activate(Task::Flow, n).unwrap();
        let v = save_as(root, &l, None, "a").await.unwrap();
        let stored = load(root).await.unwrap();
        assert_eq!(stored.doc, l);
        assert!(
            save_as(root, &l, Some(v + 5), "b").await.is_err(),
            "stale version"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
