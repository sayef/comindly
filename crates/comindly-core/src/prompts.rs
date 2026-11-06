//! The LLM prompts indexing uses: built in (v0), or an activated saved version, stored with the
//! index (`<index>/_team`, table `prompts`). Only the instructions are editable, never the format.

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

    /// The reply format the parser expects; never editable.
    pub fn format(self) -> &'static str {
        match self {
            Task::Summary => {
                "Reply in exactly this format:\n\
                SUMMARY: <one concise sentence describing what this does>\n\
                QUERIES: <3 natural-language questions a developer might ask that this code answers, separated by ` | `>\n\
                No preamble, no code fences."
            }
            Task::Guide => {
                "Output ONLY this section's rules as terse markdown bullets: NO top-level heading, \
                no preamble, no closing note. If the evidence shows no consistent convention, reply \
                with exactly: No consistent convention observed."
            }
            Task::Flow => {
                "You get an entry point and its call trace (each line: depth, relation, symbol, \
                location). Reply in exactly this format:\n\
                NARRATION: <the walkthrough>\n\
                QUERIES: <3 questions a developer might ask about this flow, separated by ` | `>\n\
                No preamble, no code fences."
            }
        }
    }
}

/// One saved version of a task's instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Version {
    pub task: Task,
    pub version: u32,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub author: String,
    pub created_at: String,
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
    pub fn add(&mut self, task: Task, text: &str, note: Option<&str>, by: &str) -> Result<u32> {
        let text = text.trim();
        anyhow::ensure!(!text.is_empty(), "the prompt is empty");
        anyhow::ensure!(
            text.chars().count() <= MAX_LEN,
            "the prompt is longer than {MAX_LEN} characters"
        );
        let version = self.of(task).map(|v| v.version).max().unwrap_or(0) + 1;
        self.versions.push(Version {
            task,
            version,
            text: text.to_string(),
            note: note
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string),
            author: by.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
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

    /// The prompts an index run uses.
    pub fn in_use(&self) -> Prompts {
        Prompts(
            self.active
                .iter()
                .filter_map(|(t, n)| Some((*t, (*n, self.find(*t, *n)?.text.clone()))))
                .collect(),
        )
    }
}

/// The instructions each task runs with, fixed for one index run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Prompts(BTreeMap<Task, (u32, String)>);

impl Prompts {
    /// Only `task` changed, to unsaved `text`, for trying it.
    pub fn draft(task: Task, text: &str) -> Prompts {
        Prompts(BTreeMap::from([(task, (0, text.trim().to_string()))]))
    }

    pub fn version(&self, task: Task) -> u32 {
        self.0.get(&task).map_or(0, |(n, _)| *n)
    }

    pub fn system(&self, task: Task) -> String {
        let text = self
            .0
            .get(&task)
            .map_or(task.builtin(), |(_, t)| t.as_str());
        format!("{text}\n\n{}", task.format())
    }

    /// Part of the reuse key of what `task` writes; `None` for v0, so built-in results stay valid.
    pub fn key(&self, task: Task) -> Option<String> {
        self.0
            .get(&task)
            .map(|(_, t)| format!("prompt\0{}", blake3::hash(t.as_bytes()).to_hex()))
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
            .add(Task::Summary, "Summarise for search.", None, "a")
            .unwrap();
        assert_eq!(
            l.in_use().version(Task::Summary),
            0,
            "a saved draft is not used"
        );
        assert_eq!(l.in_use().key(Task::Summary), None);
        l.activate(Task::Summary, v1).unwrap();
        let p = l.in_use();
        assert_eq!(p.version(Task::Summary), 1);
        assert!(p.system(Task::Summary).starts_with("Summarise for search."));
        assert!(
            p.system(Task::Summary).contains("SUMMARY:"),
            "the format stays"
        );
        assert!(p.key(Task::Summary).is_some());
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
        assert_eq!(l.add(Task::Guide, "a", None, "x").unwrap(), 1);
        assert_eq!(l.add(Task::Guide, "b", Some(" "), "x").unwrap(), 2);
        assert_eq!(l.add(Task::Flow, "c", None, "x").unwrap(), 1);
        assert_eq!(l.versions[1].note, None);
        assert!(l.add(Task::Flow, "  ", None, "x").is_err());
    }

    #[tokio::test]
    async fn saved_with_the_index() {
        let dir = std::env::temp_dir().join(format!("comindly-prompts-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        let mut l = Library::default();
        let n = l
            .add(Task::Flow, "Walk through it.", Some("shorter"), "a")
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
