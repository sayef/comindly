//! `comindly`: the command line for an engine, the one on this machine or a team's server.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;
mod doctor;
mod engine;
mod git;
mod hooks;
mod mcp;
mod profile;
mod render;
mod term;

/// Your team's engineering memory for coding agents: a cross-repo code graph and reasoning ledger.
///
/// Every command asks an engine: `local` (started on this machine when needed) or a team server.
/// `comindly use` shows and switches which one.
#[derive(Parser)]
#[command(name = "comindly", version, about, long_about = None)]
struct Cli {
    /// Run this command against another profile (`comindly use` lists them)
    #[arg(long = "use", global = true, value_name = "PROFILE")]
    profile: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// Show or switch the engine commands go to: `use local`, `use <name>`, `use <url> <name>`
    Use {
        /// `local`, a profile's name, or a server's address to add
        target: Option<String>,
        /// The name for a server added by address
        name: Option<String>,
    },
    /// The engine on this machine: start, stop, status, install
    Engine {
        #[command(subcommand)]
        what: EngineWhat,
    },
    /// Sign in to the current profile's server by approving this command line in your browser
    Login,
    /// Forget the current profile's sign-in
    Logout,
    /// Index one folder as it is, on the local engine
    Index {
        folder: String,
        /// Skip the LLM steps (summaries, walkthroughs, style guides)
        #[arg(long = "no-llm", conflicts_with = "steps")]
        no_llm: bool,
        /// Run only these LLM steps, whatever the team settings say: summary, guide, flow
        #[arg(long, value_delimiter = ',', value_name = "TASK")]
        steps: Vec<String>,
    },
    /// Index into one cross-repo index: the given folders as they are (local engine), or the
    /// engine's repository list
    Link {
        folders: Vec<String>,
        /// Skip the LLM steps (summaries, walkthroughs, style guides)
        #[arg(long = "no-llm", conflicts_with = "steps")]
        no_llm: bool,
        /// Run only these LLM steps, whatever the team settings say: summary, guide, flow
        #[arg(long, value_delimiter = ',', value_name = "TASK")]
        steps: Vec<String>,
        /// Write these LLM results again from scratch: summary, guide, flow
        #[arg(long, value_delimiter = ',', value_name = "TASK")]
        rewrite: Vec<String>,
    },
    /// Zoom, blast radius, and context pack for a symbol
    Explore { focus: String },
    /// Graph-aware hybrid code search
    Search {
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Only these repositories
        #[arg(long)]
        repo: Vec<String>,
        /// Only these teams' repositories
        #[arg(long)]
        team: Vec<String>,
        /// `table` for a table instead of markdown
        #[arg(long)]
        format: Option<String>,
    },
    /// Find symbols by name or path substring
    Find {
        query: String,
        #[arg(long)]
        repo: Vec<String>,
        #[arg(long)]
        team: Vec<String>,
    },
    /// Pre-generated flow walkthrough + live call trace
    Flow { focus: String },
    /// Index statistics per repo: symbols, edges, kinds, enrichment coverage
    Stats {
        #[arg(long)]
        repo: Vec<String>,
    },
    /// Print per-repo inferred coding style guides
    Guide {
        #[arg(long)]
        repo: Vec<String>,
        /// Write them to this file instead
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
    /// Record why code is the way it is: a decision, attempt, constraint, gotcha, intent or question
    Remember {
        kind: String,
        /// Symbols it is about
        #[arg(long)]
        on: Vec<String>,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// When a future reader needs this
        #[arg(long)]
        trigger: Option<String>,
        #[arg(long)]
        evidence: Vec<String>,
        /// Kind-specific `key=value`
        #[arg(long)]
        field: Vec<String>,
        /// Code that shaped it: `path`, `path:START-END` or `path#symbol`
        #[arg(long)]
        snippet: Vec<String>,
    },
    /// Reasoning blame: recorded decisions, failed attempts and constraints about a symbol
    Why { focus: String },
    /// Confirm, archive, invalidate, supersede, dispute or retract a ledger record
    Review {
        id: String,
        action: String,
        #[arg(long, default_value = "")]
        reason: String,
    },
    /// The ledger with derived status; `--inbox` shows only what needs review
    Ledger {
        #[arg(long)]
        inbox: bool,
        /// Run constraints' checks
        #[arg(long)]
        check: bool,
    },
    /// Erase ledger records by id
    Erase {
        #[arg(required = true)]
        ids: Vec<String>,
        /// Erase them; without it, only lists what would go
        #[arg(long)]
        yes: bool,
    },
    /// Coding conventions per repo and org-wide: measured from the code, plus the team's own
    Conventions {
        #[command(subcommand)]
        what: Option<ConventionsWhat>,
        #[arg(long, global = true)]
        repo: Option<String>,
    },
    /// Internal packages: who publishes each, who depends on it, and at which version
    Packages {
        /// Only packages used at more than one version
        #[arg(long)]
        skew: bool,
    },
    /// Service contracts (HTTP routes, queues, topics, buckets, tables) and who uses them
    Contracts {
        #[arg(long = "type")]
        kind: Option<String>,
        /// Only contracts with a missing side
        #[arg(long)]
        dangling: bool,
    },
    /// Before committing: what your uncommitted changes touch
    Preflight {
        #[arg(default_value = ".")]
        repo: String,
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Exit non-zero when something blocks
        #[arg(long)]
        strict: bool,
    },
    /// Files changed in a repository since a commit (git diff)
    Changed {
        repo: String,
        #[arg(long)]
        since: Option<String>,
    },
    /// The repositories the engine indexes: add, remove, scan
    Repos {
        #[command(subcommand)]
        what: Option<ReposWhat>,
    },
    /// Team settings shared by everyone on the index, and your own API key
    Team {
        #[command(subcommand)]
        what: Option<TeamWhat>,
    },
    /// The LLM prompts indexing uses: show, save a draft, activate, deactivate, delete
    Prompts {
        #[command(subcommand)]
        what: Option<PromptsWhat>,
    },
    /// The engine's settings: show, set
    Config {
        #[command(subcommand)]
        what: Option<ConfigWhat>,
    },
    /// The web app of the current profile: its own window on macOS, the browser elsewhere
    App,
    /// MCP over stdio for agents, forwarded to the current profile's engine
    Serve,
    /// Agent hook adapter: reads the hook's JSON on stdin, prints context to inject
    Hook { agent: String, event: String },
    /// Print the hook configuration to add to an agent's settings
    InitHooks { agent: String },
    /// Print when and how agents should use comindly's tools, for AGENTS.md or CLAUDE.md
    InitAgents {
        /// As a Claude Code skill instead
        #[arg(long)]
        skill: bool,
    },
    /// Check this machine's setup and the current profile's engine
    Doctor,
}

#[derive(Subcommand)]
pub enum EngineWhat {
    /// Start the local engine in the background
    Start,
    /// Stop the local engine
    Stop,
    /// Whether the local engine runs, and where
    Status,
    /// Download the local engine of this version
    Install,
}

#[derive(Subcommand)]
pub enum ConventionsWhat {
    /// Add the team's own rule
    Add {
        rule: String,
        #[arg(long, default_value = "team")]
        dimension: String,
    },
    /// Reword a convention
    Edit { id: String, rule: String },
    /// Hide a convention
    Rm { id: String },
    /// Undo a change to a convention
    Restore { id: String },
}

#[derive(Subcommand)]
pub enum ReposWhat {
    /// Add folders to the list
    Add {
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// Remove repositories from the list
    Remove {
        #[arg(required = true)]
        repos: Vec<String>,
    },
    /// Find git repositories under a folder
    Scan {
        dir: String,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
}

#[derive(Subcommand)]
pub enum TeamWhat {
    /// Change shared settings: KEY=VALUE, an empty value resets one
    Set {
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
    },
    /// Store your LLM API key (asked for, or read from stdin)
    Key {
        #[arg(long)]
        stdin: bool,
    },
}

#[derive(Subcommand)]
pub enum PromptsWhat {
    /// A task's prompt: the one in use, or a version
    Show {
        task: String,
        version: Option<u32>,
    },
    /// Save a file as a new version (not in use until activated)
    Save {
        task: String,
        file: String,
        #[arg(long)]
        note: Option<String>,
    },
    Activate {
        task: String,
        version: u32,
    },
    /// Back to the built-in prompt
    Deactivate {
        task: String,
    },
    Delete {
        task: String,
        version: u32,
    },
}

#[derive(Subcommand)]
pub enum ConfigWhat {
    Show,
    /// index_dir, author_name, record_names, cache_ttl_secs: KEY=VALUE
    Set {
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    commands::run(cli.profile.as_deref(), cli.cmd)
}
