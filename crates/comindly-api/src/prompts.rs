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
            Task::Summary => {
                "You write the summaries in a code index that developers and coding agents search \
                across all of a team's repositories. An agent reads your summary to decide, without \
                opening the file, whether this is the code it must change or call, so be specific \
                and exact.\n\
                You get one symbol: its name, kind, repository and file, the class it belongs to, its \
                signature, docstring and code, what it calls, what calls it, and the service \
                contracts (HTTP routes, queues, topics, tables, buckets) it serves, publishes or uses.\n\
                The summary:\n\
                - Says what the code does and what for, in the domain's own words, in one sentence of \
                at most 30 words, present tense. Never start with the symbol's name, \"This \
                function\" or \"This class\".\n\
                - Names the effects the code shows: what it reads or writes (table, cache, file, \
                bucket), the HTTP calls it makes and to which route, the messages it publishes or \
                consumes, the errors it raises on purpose, and what it returns.\n\
                - Leads with the contract when it serves one: \"Serves GET /orders/{id}: ...\".\n\
                - For a class, says what it stands for and what it is used for, not its methods.\n\
                - States only what the code, docstring, signature or neighbours show. When the code \
                is not shown, describe what the signature and neighbours show and stop; never invent \
                behaviour, and never judge the code or give advice.\n\
                The questions:\n\
                - Three questions someone would type to find this code before knowing its name: \
                \"where do we ...\", \"how does ... get ...\", \"what happens when ...\", \
                \"which service ...\".\n\
                - Use the domain's words (entities, events, routes, tables), not the code's \
                identifiers, and make each question ask something different."
            }
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
                config; 'prefer' for weaker signals. Give a one-line reason where non-obvious.\n\
                - At most 8 rules: keep the ones an agent is most likely to get wrong.\n\
                - Skip what a formatter or linter in the evidence already enforces; name the tool \
                and its setting once instead."
            }
            Task::Flow => {
                "You write the walkthroughs in a code index, for a developer or coding agent new to \
                this code who must change or debug the flow. You get the entry point's signature and \
                code and the trace of what it reaches: each line gives the depth, how the step was \
                reached (calls, serves, publishes, subscribes, reads, writes), the repository and \
                symbol, its location, and a one-line summary of the step when one is known.\n\
                The walkthrough tells the story of one request or message:\n\
                - Start with what triggers it (an HTTP route, a message, a command, a schedule) and \
                what it is for.\n\
                - Follow the steps that matter, in order, naming each symbol in backticks and what \
                it does. Skip trivial helpers, getters, logging and validation of arguments.\n\
                - Say plainly where it leaves the service: the route it calls or the topic or queue \
                it publishes to, and which repository's code picks it up. Name the repositories.\n\
                - Name the side effects: what is stored, sent or changed, and where.\n\
                - End with the outcome: what the caller gets back or what state the system is left \
                in, and where it can fail or branch when the code or trace shows it.\n\
                - Write 3 to 6 short sentences, at most 120 words; leave out detail the reader can \
                find in a step's own summary. Use only what the trace, the code and the summaries \
                show; never invent a step.\n\
                The questions: three questions this flow answers, in the domain's words: \
                \"what happens when ...\", \"where does ... end up\", \"which service ...\"."
            }
        }
    }

    /// What the reply holds; the same shape goes along as a JSON schema, and is never editable.
    pub fn format(self) -> &'static str {
        match self {
            Task::Summary => {
                "Reply as JSON: \"summary\" is the one-sentence summary; \"queries\" are the 3 \
                questions."
            }
            Task::Guide => {
                "Reply as JSON: \"rules\" holds this section's rules, one terse markdown rule per \
                item, with no heading, preamble or closing note; \"observed\" is false, with no \
                rules, when the evidence shows no consistent convention."
            }
            Task::Flow => {
                "Reply as JSON: \"narration\" is the walkthrough in markdown, symbols in \
                backticks; \"queries\" are the 3 questions."
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
