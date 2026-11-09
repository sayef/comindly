use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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
