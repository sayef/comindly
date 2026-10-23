//! Comindly CLI — the single distributable binary.
//!
//! Subcommands: `index`, `link`, `explore`, `search`, `find`, `repos`, `stats`, `guide`,
//! `flow`, `changed`, `serve`, `config`, `eval`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use comindly::model::{Edge, Symbol};

mod remote;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

/// Cross-repo code intelligence for coding agents.
///
/// `--index-dir` defaults to the configured index dir (`comindly config path`) and accepts a folder
/// or an `s3://`, `gs://` or `az://` location. The same value is used to build and to read an index.
#[derive(Parser)]
#[command(name = "comindly", version, about, long_about = None)]
struct Cli {
    /// Ask this comindly server instead of a local index (default: `COMINDLY_SERVER`, else the
    /// server `comindly login` signed you in to)
    #[arg(long, global = true)]
    server: Option<String>,
    /// Use the index on this machine even when signed in to a server
    #[arg(long, global = true, conflicts_with = "server")]
    local: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Index a single repo into the default index dir (searchable by default)
    Index {
        /// Repository path
        repo: String,
        /// Index directory — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Vector embeddings for hybrid search (default on; config `embed`)
        #[arg(long, overrides_with = "no_embed")]
        embed: bool,
        #[arg(long = "no-embed", overrides_with = "embed")]
        no_embed: bool,
        /// LLM per-symbol summaries + queries (sends code; needs OPENAI_API_KEY; config `enrich`)
        #[arg(long, overrides_with = "no_enrich")]
        enrich: bool,
        #[arg(long = "no-enrich", overrides_with = "enrich")]
        no_enrich: bool,
        /// LLM flow walkthroughs (sends call traces; needs OPENAI_API_KEY; config `flows`)
        #[arg(long, overrides_with = "no_flows")]
        flows: bool,
        #[arg(long = "no-flows", overrides_with = "flows")]
        no_flows: bool,
        /// Evidence-based style guide (sends code/config; needs OPENAI_API_KEY; config `guide`)
        #[arg(long, overrides_with = "no_guide")]
        guide: bool,
        #[arg(long = "no-guide", overrides_with = "guide")]
        no_guide: bool,
        /// Skip every LLM step (enrich, flows, guide), whatever the config says
        #[arg(long = "no-llm")]
        no_llm: bool,
        /// Reparse only files changed since the last indexed commit
        #[arg(long)]
        incremental: bool,
        /// Diff against this commit instead of the recorded base
        #[arg(long)]
        since: Option<String>,
        /// SCIP: auto (index.scip or installed indexers) | require | off; config `scip`
        #[arg(long)]
        scip: Option<String>,
    },
    /// Link several repos into one cross-repo index (adds cross-repo edges + blast radius)
    Link {
        /// Repository folders, indexed as they are; none: the configured list (`comindly repos`),
        /// each at its default branch in comindly's own worktree
        repos: Vec<String>,
        /// Index directory — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Vector embeddings for hybrid search (default on; config `embed`)
        #[arg(long, overrides_with = "no_embed")]
        embed: bool,
        #[arg(long = "no-embed", overrides_with = "embed")]
        no_embed: bool,
        /// LLM per-symbol summaries + queries (sends code; needs OPENAI_API_KEY; config `enrich`)
        #[arg(long, overrides_with = "no_enrich")]
        enrich: bool,
        #[arg(long = "no-enrich", overrides_with = "enrich")]
        no_enrich: bool,
        /// LLM flow walkthroughs (sends call traces; needs OPENAI_API_KEY; config `flows`)
        #[arg(long, overrides_with = "no_flows")]
        flows: bool,
        #[arg(long = "no-flows", overrides_with = "flows")]
        no_flows: bool,
        /// Evidence-based style guide (sends code/config; needs OPENAI_API_KEY; config `guide`)
        #[arg(long, overrides_with = "no_guide")]
        guide: bool,
        #[arg(long = "no-guide", overrides_with = "guide")]
        no_guide: bool,
        /// Skip every LLM step (enrich, flows, guide), whatever the config says
        #[arg(long = "no-llm")]
        no_llm: bool,
        /// Recompute only symbols in files changed since the last index
        #[arg(long)]
        incremental: bool,
        /// SCIP: auto (index.scip or installed indexers) | require | off; config `scip`
        #[arg(long)]
        scip: Option<String>,
    },
    /// Zoom, blast radius, and context pack for a symbol
    Explore {
        /// Symbol name or fragment to focus on
        focus: String,
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Repo paths to parse on the fly instead of reading an index
        repos: Vec<String>,
    },
    /// Graph-aware hybrid code search
    Search {
        /// Natural-language query
        #[arg(required = true, num_args = 1..)]
        query: Vec<String>,
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Restrict results to one or more repos (repeatable: --repo x --repo y)
        #[arg(long)]
        repo: Vec<String>,
        /// Output format: `md` or `table` (default from config, else `md`)
        #[arg(long)]
        format: Option<String>,
        /// Shortcut for `--format md`
        #[arg(long)]
        md: bool,
    },
    /// Files changed since a commit (git diff)
    Changed {
        /// Repository path
        repo: String,
        /// Base commit (default: HEAD's parent)
        #[arg(long)]
        since: Option<String>,
    },
    /// Pre-generated flow walkthrough + live call trace
    Flow {
        /// Entry-point symbol
        focus: String,
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Run the MCP server over stdio
    Serve {
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Output format handed to the agent: `md` (default) or `json`
        #[arg(long)]
        format: Option<String>,
        /// Shortcut for `--format json`
        #[arg(long)]
        json: bool,
    },
    /// Find symbols by name or path substring
    Find {
        /// Name or descriptor/path substring
        query: String,
        /// Index directory to read — root; default: configured index dir
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Limit to one or more repos (repeatable: --repo x --repo y)
        #[arg(long)]
        repo: Vec<String>,
    },
    /// Indexed repositories with their symbol counts, and the list `comindly link` indexes when
    /// given no folders (`add`, `remove`, `scan`)
    Repos {
        #[command(subcommand)]
        what: Option<ReposWhat>,
        /// Index directory to read — root; default: configured index dir
        #[arg(long = "index-dir", global = true)]
        index_dir: Option<String>,
    },
    /// Index statistics per repo: symbols, edges, kinds, enrichment coverage
    Stats {
        /// Index directory to read — root; default: configured index dir
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Limit to one or more repos (repeatable: --repo x --repo y). Omit for all repos.
        #[arg(long)]
        repo: Vec<String>,
    },
    /// Print per-repo inferred coding style guides (built with --guide)
    Guide {
        /// Index directory to read — root; default: configured index dir
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Limit to one or more repos (repeatable: --repo x --repo y)
        #[arg(long)]
        repo: Vec<String>,
        /// Write the guide(s) to a file instead of the terminal
        #[arg(long, short = 'o')]
        output: Option<String>,
    },
    /// Record why code is the way it is: a decision, failed attempt, constraint, gotcha, intent or question
    Remember {
        /// decision | attempt | constraint | gotcha | intent | question
        kind: String,
        /// The symbol (or file) this is about (repeatable)
        #[arg(long)]
        on: Vec<String>,
        /// One-line statement of the fact
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        /// When a future reader needs this
        #[arg(long)]
        trigger: Option<String>,
        /// Evidence: commit:<sha>, mr:!123, test:<path>::<name>, url:…, or text (repeatable)
        #[arg(long)]
        evidence: Vec<String>,
        /// Kind-specific field `key=value` (repeatable), e.g. outcome=reverted
        #[arg(long)]
        field: Vec<String>,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Reasoning blame: recorded decisions, failed attempts and constraints about a symbol
    Why {
        focus: String,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Confirm, archive, invalidate, supersede, dispute or retract a ledger record
    Review {
        id: String,
        /// confirm | archive (no longer applies) | invalidate (was wrong) | supersede | dispute | retract
        action: String,
        #[arg(long, default_value = "")]
        reason: String,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// The ledger with derived status; `--inbox` shows only what needs review
    Ledger {
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Only stale, orphaned, or unconfirmed agent records
        #[arg(long)]
        inbox: bool,
        /// Evaluate constraint checks; exits non-zero when a `must` constraint is violated (CI)
        #[arg(long)]
        check: bool,
        /// Fold new inbox records into the Lance snapshot and drop erased ones
        #[arg(long)]
        compact: bool,
    },
    /// Erase ledger records (and relations about them) by id or by author; dry run without --yes
    Erase {
        /// Record ids or id prefixes
        ids: Vec<String>,
        /// Every record by this pseudonymous actor (`me` for yours)
        #[arg(long)]
        actor: Option<String>,
        /// Delete for real
        #[arg(long)]
        yes: bool,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Terminal UI (or `--web`: local web UI) for overview, contracts, ledger, explorer and conventions
    Ui {
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Serve the web UI instead of the terminal UI
        #[arg(long)]
        web: bool,
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// With `--web`: the address to listen on (`0.0.0.0` in a container)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// With `--web`: open it in the browser
        #[arg(long)]
        open: bool,
    },
    /// Team settings shared by everyone using this index (LLM, embedding model), and your own API key
    Team {
        #[command(subcommand)]
        what: Option<TeamWhat>,
        #[arg(long = "index-dir", global = true)]
        index_dir: Option<String>,
    },
    /// Use a team's index on this machine: remember its location
    Join {
        /// The team's index (a folder, s3://, gs:// or az:// location)
        index: String,
    },
    /// The web UI in its own window (macOS)
    App {
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Before committing: recorded constraints/dead ends, contracts, blast radius and missing
    /// companions for your uncommitted changes
    Preflight {
        /// Repository path (default: current directory)
        #[arg(default_value = ".")]
        repo: String,
        /// Compare the working tree against this revision
        #[arg(long, default_value = "HEAD")]
        base: String,
        /// Exit non-zero when a `must` constraint check fails in a changed file (CI)
        #[arg(long)]
        strict: bool,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Check this machine's setup: index, ledger, hooks, keys, signing, SCIP indexers
    Doctor {
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Local mirror of a remote index (used by hooks)
    Cache {
        #[command(subcommand)]
        what: CacheWhat,
    },
    /// Agent hook adapter: reads the hook's JSON on stdin, prints context to inject (stdout JSON)
    Hook {
        /// Agent: `claude` (JSON for Claude Code), or `cursor` | `gemini` | `codex` | `generic`
        /// (plain markdown on stdout; the edited file is found in the payload or `COMINDLY_HOOK_FILE`)
        agent: String,
        /// Event: `session-start` | `pre-edit`
        event: String,
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
    /// Print the hook configuration to add to an agent's settings
    InitHooks {
        /// Agent: `claude` (JSON for Claude Code), or `cursor` | `gemini` | `codex` | `generic`
        /// (plain markdown on stdout; the edited file is found in the payload or `COMINDLY_HOOK_FILE`)
        agent: String,
    },
    /// Coding conventions per repo and org-wide: measured from the code, plus the team's own
    Conventions {
        #[command(subcommand)]
        what: Option<ConventionsWhat>,
        /// Index directory — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir", global = true)]
        index_dir: Option<String>,
        /// Only this repo (and the org baseline); with `add`, the repo it applies to (default: org-wide)
        #[arg(long, global = true)]
        repo: Option<String>,
    },
    /// Internal packages: who publishes each, who depends on it, and at which version
    Packages {
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Only consumers pinned to a version other than the publisher's
        #[arg(long)]
        skew: bool,
    },
    /// Service contracts (HTTP routes, queues, topics, buckets, tables) and who uses them
    Contracts {
        /// Index directory to read — root; default: configured index dir (a folder, or an s3://, gs:// or az:// location)
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
        /// Only contracts of this type: http, sqs, sns, s3, table
        #[arg(long = "type")]
        kind: Option<String>,
        /// Only contracts missing a provider or a consumer
        #[arg(long)]
        dangling: bool,
    },
    /// Measure graph quality against a gold set
    Eval {
        #[command(subcommand)]
        what: EvalWhat,
    },
    /// Sign in to a comindly server by approving this command line in your browser
    Login {
        /// The server's address, e.g. https://comindly.example.com
        url: String,
    },
    /// Forget the server sign-in on this machine
    Logout,
    /// Your preferences on this machine: show, set, import, export, history
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(Subcommand)]
enum TeamWhat {
    /// Set team settings, KEY=VALUE: name, llm_model, llm_base_url, embed_model, embed, enrich, flows, guide, max_enrich, max_flows, llm_concurrency, llm_price_in, llm_price_out, format, scip, auto_index (empty removes)
    Set {
        #[arg(required = true, value_name = "KEY=VALUE")]
        pairs: Vec<String>,
    },
    /// Store your LLM API key for this team in ~/.config/comindly/credentials (never shared)
    Key {
        /// Read the key from stdin instead of prompting
        #[arg(long)]
        stdin: bool,
        /// Store it as your default key, for every index
        #[arg(long)]
        default: bool,
    },
}

#[derive(Subcommand)]
enum ReposWhat {
    /// Add repository folders (your clones; comindly indexes their default branch)
    Add {
        #[arg(required = true)]
        paths: Vec<String>,
    },
    /// Remove repositories by name or folder
    Remove {
        #[arg(required = true)]
        repos: Vec<String>,
    },
    /// List git repositories under a folder
    Scan {
        dir: String,
        #[arg(long, default_value_t = 2)]
        depth: usize,
    },
}

#[derive(Subcommand)]
enum ConventionsWhat {
    /// Add a convention
    Add {
        rule: String,
        /// e.g. naming, errors, testing, libraries
        #[arg(long, default_value = "team")]
        dimension: String,
    },
    /// Rewrite a convention's rule (id from the listing)
    Edit { id: String, rule: String },
    /// Remove a convention; a measured one is hidden and can be restored
    Rm { id: String },
    /// Bring back a hidden or edited measured convention as measured
    Restore { id: String },
}

#[derive(Subcommand)]
enum CacheWhat {
    /// Bring the local mirror of a remote index up to date
    Refresh {
        #[arg(long = "index-dir")]
        index_dir: Option<String>,
    },
}

#[derive(Subcommand)]
enum EvalWhat {
    /// Precision/recall of resolved edges on a fixture org (subdir per repo + expected.toml)
    Graph {
        /// Fixture directory
        #[arg(default_value = "tests/fixtures/org")]
        fixture: String,
        /// List every missed and spurious edge
        #[arg(long, short)]
        verbose: bool,
        /// Directory of `<repo>.scip` indexes to attach
        #[arg(long)]
        scip: Option<String>,
        /// Limit to these fixture repos (repeatable)
        #[arg(long)]
        repo: Vec<String>,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Show your preferences (the default)
    Show,
    /// Set preferences: KEY=VALUE (index_dir, author_name, record_names, signing_key, allowed_signers, cache_ttl_secs); empty removes
    Set { pairs: Vec<String> },
    /// Apply a TOML file in the shape `export` writes; fills what is unset unless --override
    Import {
        file: String,
        /// Replace your preferences with the file's
        #[arg(long = "override")]
        replace: bool,
    },
    /// Write your preferences as TOML, to a file or stdout
    Export { file: Option<String> },
    /// Earlier versions of your preferences
    History,
    /// Where your preferences are kept
    Path,
}

fn main() -> ExitCode {
    let matches = Cli::command().get_matches();
    // An older config.toml is imported once; a server keeps its settings in its database.
    if std::env::var_os("COMINDLY_PUBLIC_URL").is_none() {
        comindly::config::adopt_file();
    }
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    if let Some(c) = remote::client(cli.server.as_deref(), cli.local) {
        if let Some(code) = remote::dispatch(&c, &cli.cmd) {
            return code;
        }
    }
    match cli.cmd {
        Cmd::Index {
            repo,
            index_dir,
            embed,
            no_embed,
            enrich,
            no_enrich,
            flows,
            no_flows,
            guide,
            no_guide,
            no_llm,
            incremental,
            since,
            scip,
        } => {
            let s = settings_for(index_dir.as_deref());
            let Some(scip) = scip_mode(scip.as_deref(), s.scip) else {
                return ExitCode::FAILURE;
            };
            cmd_index(
                &repo,
                index_dir.as_deref(),
                tri(embed, no_embed).unwrap_or(s.embed),
                !no_llm && tri(enrich, no_enrich).unwrap_or(s.enrich),
                !no_llm && tri(flows, no_flows).unwrap_or(s.flows),
                !no_llm && tri(guide, no_guide).unwrap_or(s.guide),
                incremental,
                since.as_deref(),
                scip,
            )
        }
        Cmd::Link {
            repos,
            index_dir,
            embed,
            no_embed,
            enrich,
            no_enrich,
            flows,
            no_flows,
            guide,
            no_guide,
            no_llm,
            incremental,
            scip,
        } => {
            let s = settings_for(index_dir.as_deref());
            let Some(scip) = scip_mode(scip.as_deref(), s.scip) else {
                return ExitCode::FAILURE;
            };
            cmd_link(
                &repos,
                index_dir.as_deref(),
                tri(embed, no_embed).unwrap_or(s.embed),
                !no_llm && tri(enrich, no_enrich).unwrap_or(s.enrich),
                !no_llm && tri(flows, no_flows).unwrap_or(s.flows),
                !no_llm && tri(guide, no_guide).unwrap_or(s.guide),
                incremental,
                scip,
            )
        }
        Cmd::Explore {
            focus,
            index_dir,
            repos,
        } => {
            let repos: Vec<&str> = repos.iter().map(String::as_str).collect();
            cmd_explore(&focus, index_dir.as_deref(), &repos)
        }
        Cmd::Search {
            query,
            index_dir,
            repo,
            format,
            md,
        } => {
            // CLI flag → config → default. `md`/`markdown` = markdown; anything else = table.
            let fmt = if md { Some("md".to_string()) } else { format };
            let fmt = fmt.unwrap_or_else(|| settings_for(index_dir.as_deref()).format);
            let markdown = matches!(fmt.as_str(), "md" | "markdown");
            cmd_search(&query.join(" "), index_dir.as_deref(), &repo, markdown)
        }
        Cmd::Find {
            query,
            index_dir,
            repo,
        } => cmd_find(&query, index_dir.as_deref(), &repo),
        Cmd::Repos { what, index_dir } => cmd_repo_list(what, index_dir.as_deref()),
        Cmd::Stats { index_dir, repo } => {
            let repos: Vec<&str> = repo.iter().map(String::as_str).collect();
            cmd_stats(&repos, index_dir.as_deref())
        }
        Cmd::Guide {
            index_dir,
            repo,
            output,
        } => {
            let repos: Vec<&str> = repo.iter().map(String::as_str).collect();
            cmd_guide(&repos, index_dir.as_deref(), output.as_deref())
        }
        Cmd::Changed { repo, since } => cmd_changed(&repo, since.as_deref()),
        Cmd::Flow { focus, index_dir } => cmd_flow(&focus, index_dir.as_deref()),
        Cmd::Serve {
            index_dir,
            format,
            json,
        } => {
            // CLI flag → config → default. `json` = raw JSON; anything else = markdown.
            let fmt = if json {
                Some("json".to_string())
            } else {
                format
            };
            let fmt = fmt.unwrap_or_else(|| settings_for(index_dir.as_deref()).format);
            let markdown = !matches!(fmt.as_str(), "json");
            cmd_serve(index_dir.as_deref(), markdown)
        }
        Cmd::Contracts {
            index_dir,
            kind,
            dangling,
        } => cmd_contracts(index_dir.as_deref(), kind.as_deref(), dangling),
        Cmd::Packages { index_dir, skew } => cmd_packages(index_dir.as_deref(), skew),
        Cmd::Conventions {
            what: None,
            index_dir,
            repo,
        } => cmd_conventions(index_dir.as_deref(), repo.as_deref()),
        Cmd::Conventions {
            what: Some(what),
            index_dir,
            repo,
        } => cmd_conventions_change(index_dir.as_deref(), repo.as_deref(), what),
        Cmd::Remember {
            kind,
            on,
            title,
            body,
            trigger,
            evidence,
            field,
            index_dir,
        } => cmd_remember(
            &kind,
            &on,
            &title,
            &body,
            trigger,
            evidence,
            &field,
            index_dir.as_deref(),
        ),
        Cmd::Why { focus, index_dir } => cmd_why(&focus, index_dir.as_deref()),
        Cmd::Review {
            id,
            action,
            reason,
            index_dir,
        } => cmd_review(&id, &action, &reason, index_dir.as_deref()),
        Cmd::Ledger {
            index_dir,
            inbox,
            check,
            compact,
        } => cmd_ledger(index_dir.as_deref(), inbox, check, compact),
        Cmd::Hook {
            agent,
            event,
            index_dir,
        } => cmd_hook(&agent, &event, index_dir.as_deref()),
        Cmd::InitHooks { agent } => cmd_init_hooks(&agent),
        Cmd::Doctor { index_dir } => cmd_doctor(index_dir.as_deref()),
        Cmd::Cache {
            what: CacheWhat::Refresh { index_dir },
        } => {
            let root = comindly::config::Config::load().index_dir(index_dir.as_deref());
            let Some(dir) = comindly::cache::dir(&root) else {
                comindly::ui::err("no HOME for the cache");
                return ExitCode::FAILURE;
            };
            match comindly::cache::refresh(&root, &dir) {
                Ok(copied) => {
                    comindly::ui::ok(if copied {
                        "mirror updated (graph copied)"
                    } else {
                        "mirror up to date"
                    });
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
                    ExitCode::FAILURE
                }
            }
        }
        Cmd::Preflight {
            repo,
            base,
            strict,
            index_dir,
        } => cmd_preflight(&repo, &base, strict, index_dir.as_deref()),
        Cmd::Ui {
            index_dir,
            web,
            port,
            host,
            open,
        } => cmd_ui(index_dir.as_deref(), web.then_some((host, port)), open),
        Cmd::App { index_dir } => cmd_app(index_dir.as_deref()),
        Cmd::Team { what, index_dir } => cmd_team(what, index_dir.as_deref()),
        Cmd::Join { index } => cmd_join(&index),
        Cmd::Erase {
            ids,
            actor,
            yes,
            index_dir,
        } => cmd_erase(&ids, actor.as_deref(), yes, index_dir.as_deref()),

        Cmd::Eval {
            what:
                EvalWhat::Graph {
                    fixture,
                    verbose,
                    scip,
                    repo,
                },
        } => cmd_eval_graph(&fixture, verbose, scip.as_deref(), &repo),
        Cmd::Config { action } => cmd_config(action),
        Cmd::Login { url } => cmd_login(&url),
        Cmd::Logout => cmd_logout(),
    }
}

/// `--scip` flag → config → `auto`; `None` (after an error message) for an unknown value.
fn scip_mode(flag: Option<&str>, default: comindly::scip::Mode) -> Option<comindly::scip::Mode> {
    match flag {
        None => Some(default),
        Some(f) => comindly::scip::Mode::parse(f).or_else(|| {
            comindly::ui::err(&format!("--scip must be auto, require or off (got {f:?})"));
            None
        }),
    }
}

/// Resolve a `--x` / `--no-x` flag pair to an explicit choice, or `None` to fall back to config.
/// clap's `overrides_with` guarantees at most one is set.
fn tri(on: bool, off: bool) -> Option<bool> {
    if on {
        Some(true)
    } else if off {
        Some(false)
    } else {
        None
    }
}

/// Parse and cross-repo-resolve a set of repos into a merged corpus.
fn parse_and_resolve(repos: &[&str]) -> Result<(Vec<Symbol>, Vec<Edge>), String> {
    let mut symbols: Vec<Symbol> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut facts = Vec::new();
    for path in repos {
        let name = repo_name(path);
        let o = comindly::parse::parse_repo(Path::new(path), &name)
            .map_err(|e| format!("{name}: parse failed: {e:#}"))?;
        symbols.extend(o.symbols);
        edges.extend(o.edges);
        facts.extend(o.facts);
    }
    let resolved = comindly::resolve::resolve(&symbols, &edges, &facts);
    symbols.extend(resolved.symbols);
    Ok((symbols, resolved.edges))
}

/// `comindly explore <focus> <repo>...` — the agent-facing view: zoom, blast radius, and a
/// token-budgeted context pack for one symbol, across the whole corpus.
fn cmd_explore(focus: &str, index_dir: Option<&str>, repos: &[&str]) -> ExitCode {
    // With repo paths and no --index-dir, parse them on the fly; otherwise read the prebuilt
    // index (flag or configured default — comindly resolves the internal dataset path).
    let (symbols, edges) = if index_dir.is_none() && !repos.is_empty() {
        match parse_and_resolve(repos) {
            Ok(x) => x,
            Err(e) => {
                comindly::ui::err(&e);
                return ExitCode::FAILURE;
            }
        }
    } else {
        let uri = comindly::config::Config::load().graph_dir(index_dir);
        match comindly::index::read_graph_blocking(&uri) {
            Ok(x) => x,
            Err(e) => {
                comindly::ui::err(&friendly_load_err(&uri, &e));
                return ExitCode::FAILURE;
            }
        }
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let dangling = if g.dangling_edges > 0 {
        format!(", {} dangling", g.dangling_edges)
    } else {
        String::new()
    };
    comindly::ui::note(&format!(
        "graph: {} symbols, {} edges{dangling}",
        g.node_count(),
        g.edge_count(),
    ));

    let Some(idx) = g.lookup(focus) else {
        comindly::ui::err(&format!("no symbol matching `{focus}`"));
        return ExitCode::FAILURE;
    };

    // zoom
    let z = g.zoom(idx);
    if let Some(f) = &z.focus {
        comindly::ui::header(&format!(
            "● {} [{}]  {}  {}",
            f.name, f.kind, f.repo, f.location
        ));
        if let Some(sig) = &f.signature {
            comindly::ui::note(sig);
        }
    }
    if let Some(c) = &z.container {
        // Drop the redundant location when the container is the file itself.
        if c.kind.eq_ignore_ascii_case("file") {
            comindly::ui::field("in", &c.name);
        } else {
            comindly::ui::field("in", &format!("{}  ({})", c.name, c.location));
        }
    }
    print_group("calls", &z.callees, 8);
    print_group("called by", &z.callers, 8);
    print_group("imported by", &z.importers, 8);
    print_group("members", &z.members, 8);

    // ripple — blast radius grouped by repo
    let hits = g.ripple(idx, 4);
    let mut by_repo: BTreeMap<String, usize> = BTreeMap::new();
    for h in &hits {
        *by_repo.entry(h.node.repo.clone()).or_default() += 1;
    }
    comindly::ui::header(&format!(
        "ripple (blast radius, ≤4 hops): {} dependents",
        hits.len()
    ));
    if !by_repo.is_empty() {
        let mut t = comindly::ui::table(&["repo", "dependents"]);
        for (repo, n) in &by_repo {
            t.add_row(vec![repo.clone(), n.to_string()]);
        }
        comindly::ui::right_align(&mut t, &[1]); // dependents
        println!("{t}");
    }

    // context pack — the minimal correct read-set within a token budget
    const BUDGET: usize = 1500;
    let pack = g.context_pack(idx, BUDGET);
    let total: usize = pack.iter().map(|(_, t)| t).sum();
    comindly::ui::header(&format!(
        "context pack for changing `{}` (~{total} tokens, budget {BUDGET}): {}",
        z.focus.as_ref().map_or("?", |f| f.name.as_str()),
        plural(pack.len(), "symbol")
    ));
    if !pack.is_empty() {
        let mut t = comindly::ui::table(&["symbol", "location"]);
        for (node, _) in pack.iter().take(15) {
            t.add_row(vec![node.name.clone(), node.location.clone()]);
        }
        println!("{t}");
    }
    ExitCode::SUCCESS
}

fn print_group(label: &str, nodes: &[comindly::graph::Node], limit: usize) {
    if nodes.is_empty() {
        return;
    }
    let names: Vec<String> = nodes.iter().take(limit).map(|n| n.name.clone()).collect();
    let more = nodes.len().saturating_sub(limit);
    let suffix = if more > 0 {
        format!(" (+{more})")
    } else {
        String::new()
    };
    comindly::ui::field(label, &format!("{}{suffix}", names.join(", ")));
}

fn cmd_search(query: &str, from: Option<&str>, repos: &[String], markdown: bool) -> ExitCode {
    let uri = comindly::config::Config::load().graph_dir(from);
    let uri = uri.as_str();

    let (symbols, edges) = match comindly::index::read_graph_blocking(uri) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&friendly_load_err(uri, &e));
            return ExitCode::FAILURE;
        }
    };
    let by_id: HashMap<String, Symbol> =
        symbols.iter().map(|s| (s.id.render(), s.clone())).collect();
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);

    let model = comindly::rt::block_on(comindly::config::index_settings(
        uri.trim_end_matches("/_graph"),
    ))
    .embed_model;
    let embedder = match comindly::embed::Embedder::load(&model) {
        Ok(e) => e,
        Err(e) => {
            comindly::ui::err(&format!("embedding model: {e:#}"));
            return ExitCode::FAILURE;
        }
    };

    // LLM enrichment (summaries + generated queries), if present — folded into ranking + shown.
    let enrich: HashMap<String, (String, Vec<String>)> =
        comindly::index::read_enrichment_blocking(uri)
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, s, q)| (id.render(), (s, q)))
            .collect();

    // Native LanceDB hybrid retrieval (BM25 + vector, RRF-fused) + comindly's code-aware boosts
    // and dependency-graph centrality — shared with the `search` MCP tool.
    let limit = comindly::answers::search_limit(repos);
    let hits = match comindly::search::hybrid_blocking(
        uri, &by_id, &g, &enrich, &embedder, query, limit,
    ) {
        Ok(h) => h,
        Err(e) => {
            comindly::ui::err(&format!(
                "hybrid search failed (was the index built with --no-embed?): {e:#}"
            ));
            return ExitCode::FAILURE;
        }
    };
    render_search(
        query,
        &comindly::answers::search(query, hits, repos),
        markdown,
    )
}

fn render_search(query: &str, s: &comindly::answers::Search, markdown: bool) -> ExitCode {
    if markdown {
        print!("{}", s.markdown);
        return ExitCode::SUCCESS;
    }
    comindly::ui::header(&format!("search: \"{query}\""));
    if s.hits.is_empty() {
        comindly::ui::note(
            "no matches — try a broader query (or the index was built with --no-embed)",
        );
        return ExitCode::SUCCESS;
    }
    let mut t = comindly::ui::table(&["score", "symbol", "kind", "deps", "location", "summary"]);
    for h in &s.hits {
        t.add_row(vec![
            format!("{:.3}", h.score),
            h.name.clone(),
            h.kind.to_string(),
            h.deps.to_string(),
            h.location.clone(),
            h.summary.clone().unwrap_or_default(),
        ]);
    }
    comindly::ui::right_align(&mut t, &[0, 3]); // score, deps
    println!("{t}");
    ExitCode::SUCCESS
}

/// Load the prebuilt graph for a read-only command, with a friendly missing-index error.
fn read_graph_or_err(from: Option<&str>) -> Result<(Vec<Symbol>, Vec<Edge>), ()> {
    let uri = comindly::config::Config::load().graph_dir(from);
    comindly::index::read_graph_blocking(&uri)
        .map_err(|e| comindly::ui::err(&friendly_load_err(&uri, &e)))
}

/// `comindly find <query>` — locate symbols by name/descriptor/path substring.
fn cmd_find(query: &str, from: Option<&str>, repos: &[String]) -> ExitCode {
    let Ok((symbols, edges)) = read_graph_or_err(from) else {
        return ExitCode::FAILURE;
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    render_find(query, &comindly::answers::find(&g, query, repos))
}

fn render_find(query: &str, hits: &[comindly::graph::Node]) -> ExitCode {
    comindly::ui::header(&format!(
        "find: \"{query}\" ({})",
        plural(hits.len(), "match")
    ));
    if hits.is_empty() {
        comindly::ui::note("no symbols matched");
        return ExitCode::SUCCESS;
    }
    let mut t = comindly::ui::table(&["symbol", "kind", "repo", "location", "id"]);
    for n in hits {
        t.add_row(vec![
            n.name.clone(),
            n.kind.clone(),
            n.repo.clone(),
            n.location.clone(),
            n.id.clone(),
        ]);
    }
    println!("{t}");
    ExitCode::SUCCESS
}

/// `comindly repos` — list indexed repositories and their symbol counts.
fn cmd_repos(from: Option<&str>) -> ExitCode {
    let Ok((symbols, edges)) = read_graph_or_err(from) else {
        return ExitCode::FAILURE;
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let repos = g.repos();
    comindly::ui::header(&format!("Indexed repositories ({})", repos.len()));
    if repos.is_empty() {
        comindly::ui::note("none");
        return ExitCode::SUCCESS;
    }
    let mut t = comindly::ui::table(&["repo", "symbols"]);
    for (r, n) in &repos {
        t.add_row(vec![r.clone(), n.to_string()]);
    }
    comindly::ui::right_align(&mut t, &[1]);
    println!("{t}");
    ExitCode::SUCCESS
}

/// `comindly stats [--repo x --repo y]` — per-repo symbols, edges, kinds, enrichment coverage.
/// With no `--repo`, reports every indexed repo separately.
fn cmd_stats(want: &[&str], from: Option<&str>) -> ExitCode {
    let Ok((symbols, edges)) = read_graph_or_err(from) else {
        return ExitCode::FAILURE;
    };
    // Ids of enriched symbols, to compute per-repo coverage.
    let uri = comindly::config::Config::load().graph_dir(from);
    let enriched: HashSet<String> = comindly::index::read_enrichment_blocking(&uri)
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .map(|(id, _, _)| id.render())
        .collect();

    // All repos present, sorted.
    let all: BTreeSet<String> = symbols.iter().map(|s| s.id.package.clone()).collect();
    // Which repos to report: the requested ones (warn on unknown) or all.
    let repos: Vec<String> = if want.is_empty() {
        all.iter().cloned().collect()
    } else {
        for r in want {
            if !all.contains(*r) {
                comindly::ui::warn(&format!("no such repo in the index: {r}"));
            }
        }
        want.iter()
            .filter(|r| all.contains(**r))
            .map(|r| r.to_string())
            .collect()
    };
    if repos.is_empty() {
        comindly::ui::err("no matching repos");
        return ExitCode::FAILURE;
    }

    comindly::ui::header(&format!(
        "Index stats — {} across {}",
        plural(symbols.len(), "symbol"),
        plural(all.len(), "repo")
    ));

    for repo in &repos {
        let rsyms: Vec<&Symbol> = symbols.iter().filter(|s| &s.id.package == repo).collect();
        let redges = edges.iter().filter(|e| &e.src.package == repo).count();
        let renr = rsyms
            .iter()
            .filter(|s| enriched.contains(&s.id.render()))
            .count();
        comindly::ui::header(&format!("● {repo}"));
        comindly::ui::field("symbols", &rsyms.len().to_string());
        comindly::ui::field("edges", &redges.to_string());
        comindly::ui::field("enriched", &format!("{renr} / {}", rsyms.len()));

        let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
        for s in &rsyms {
            *by_kind.entry(format!("{:?}", s.kind)).or_default() += 1;
        }
        let mut kt = comindly::ui::table(&["kind", "count"]);
        for (k, n) in &by_kind {
            kt.add_row(vec![k.clone(), n.to_string()]);
        }
        comindly::ui::right_align(&mut kt, &[1]);
        println!("{kt}");
    }
    ExitCode::SUCCESS
}

/// `comindly guide [--repo x --repo y] [--output f]` — per-repo inferred coding style guides
/// (built with `--enrich`). With no `--repo`, prints every repo's guide; `--output` writes to a
/// file instead of the terminal.
fn cmd_guide(want: &[&str], from: Option<&str>, output: Option<&str>) -> ExitCode {
    let uri = comindly::config::Config::load().graph_dir(from);
    let guides = match comindly::index::read_style_guide_blocking(&uri) {
        Ok(g) => g,
        Err(e) => {
            comindly::ui::err(&friendly_load_err(&uri, &e));
            return ExitCode::FAILURE;
        }
    };
    if guides.is_empty() {
        comindly::ui::note("no style guide yet — build the index with --enrich");
        return ExitCode::SUCCESS;
    }
    for r in want {
        if !guides.iter().any(|(repo, _)| repo.eq_ignore_ascii_case(r)) {
            comindly::ui::warn(&format!("no style guide for repo: {r}"));
        }
    }
    let selected: Vec<&(String, String)> = guides
        .iter()
        .filter(|(repo, _)| want.is_empty() || want.iter().any(|r| repo.eq_ignore_ascii_case(r)))
        .collect();
    if selected.is_empty() {
        comindly::ui::err("no matching repos");
        return ExitCode::FAILURE;
    }
    if let Some(path) = output {
        let doc: String = selected
            .iter()
            .map(|(repo, guide)| format!("# Style guide — {repo}\n\n{guide}\n"))
            .collect::<Vec<_>>()
            .join("\n");
        match std::fs::write(path, doc) {
            Ok(()) => comindly::ui::ok(&format!(
                "style guide{} written to {path}",
                if selected.len() > 1 { "s" } else { "" }
            )),
            Err(e) => {
                comindly::ui::err(&format!("write {path} failed: {e}"));
                return ExitCode::FAILURE;
            }
        }
    } else {
        for (repo, guide) in selected {
            comindly::ui::header(&format!("Style guide — {repo}"));
            println!("{guide}");
        }
    }
    ExitCode::SUCCESS
}

/// `comindly changed <repo> [--since <sha>]` — show files changed since a commit (incremental
/// indexing targeting). With no `--since`, just prints the current HEAD commit.
fn cmd_changed(repo: &str, since: Option<&str>) -> ExitCode {
    let path = Path::new(repo);

    let head = match comindly::git::head_commit(path) {
        Ok(h) => h,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
    };
    comindly::ui::field("HEAD", &head);

    let Some(base) = since else {
        comindly::ui::note("pass --since <commit> to list changed files");
        return ExitCode::SUCCESS;
    };
    match comindly::git::changed_files(path, base) {
        Ok(cs) => {
            comindly::ui::header(&format!(
                "changed since {base}: {} files ({} added, {} modified, {} deleted)",
                cs.total(),
                cs.added.len(),
                cs.modified.len(),
                cs.deleted.len()
            ));
            for p in cs.added.iter().take(10) {
                comindly::ui::note(&format!("+ {p}"));
            }
            for p in cs.modified.iter().take(10) {
                comindly::ui::note(&format!("~ {p}"));
            }
            for p in cs.deleted.iter().take(10) {
                comindly::ui::note(&format!("- {p}"));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

/// `comindly serve --from <lancedb-uri>` — run the MCP server over stdio. Only the MCP
/// protocol goes to stdout; diagnostics go to stderr so the stream stays clean.
fn cmd_serve(from: Option<&str>, markdown: bool) -> ExitCode {
    let uri = comindly::config::Config::load().graph_dir(from);
    let uri = uri.as_str();
    comindly::ui::step(&format!("loading graph from {uri}"));
    let rt = comindly::rt::shared();
    match rt.block_on(comindly::mcp::serve_stdio(uri, markdown)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly::ui::err(&friendly_load_err(uri, &e));
            ExitCode::FAILURE
        }
    }
}

/// `comindly config <path|init>` — show or scaffold the config file.
fn human() -> comindly::ledger::Provenance {
    comindly::ledger::Provenance::human("cli", &comindly::config::local_identity())
}

#[allow(clippy::too_many_arguments)] // CLI dispatch: flags map 1:1 to clap args
fn cmd_remember(
    kind: &str,
    on: &[String],
    title: &str,
    body: &str,
    trigger: Option<String>,
    evidence: Vec<String>,
    fields: &[String],
    from: Option<&str>,
) -> ExitCode {
    let Some(kind) = comindly::ledger::Kind::parse(kind) else {
        comindly::ui::err("kind must be decision, attempt, constraint, gotcha, intent or question");
        return ExitCode::FAILURE;
    };
    let cfg = comindly::config::Config::load();
    let (symbols, edges) = match comindly::index::read_graph_blocking(&cfg.graph_dir(from)) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&format!(
                "no index to anchor to: {}",
                comindly::storage::explain(&format!("{e:#}"))
            ));
            return ExitCode::FAILURE;
        }
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let by_id: HashMap<String, &Symbol> = symbols.iter().map(|s| (s.id.render(), s)).collect();
    let mut anchors = Vec::new();
    for focus in on {
        match g.lookup(focus).and_then(|i| by_id.get(&g.node(i).id)) {
            Some(s) => anchors.push(comindly::ledger::Anchor::of(s)),
            None => {
                comindly::ui::err(&format!(
                    "no symbol matches `{focus}` (try `comindly find`)"
                ));
                return ExitCode::FAILURE;
            }
        }
    }
    let mut rec = comindly::ledger::Record::new(kind, title, anchors, human());
    rec.body = body.to_string();
    rec.trigger = trigger;
    rec.evidence = evidence;
    for f in fields {
        if let Some((k, v)) = f.split_once('=') {
            rec.fields.insert(
                k.trim().to_string(),
                serde_json::Value::String(v.trim().into()),
            );
        }
    }
    match comindly::ledger::put_blocking(
        &cfg.index_dir(from),
        &rec,
        comindly::config::local_signing_key(),
    ) {
        Ok(()) => {
            comindly::cache::invalidate(&cfg.index_dir(from));
            comindly::ui::ok(&format!(
                "recorded {} `{}` ({})",
                kind.as_str(),
                rec.title,
                rec.id
            ));
            ExitCode::SUCCESS
        }
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn cmd_why(focus: &str, from: Option<&str>) -> ExitCode {
    let cfg = comindly::config::Config::load();
    let (symbols, edges) = match comindly::index::read_graph_blocking(&cfg.graph_dir(from)) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&format!("no index: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let records = comindly::ledger::all_blocking(&cfg.index_dir(from)).unwrap_or_default();
    let entries = comindly::ledger::derive(&records, &symbols, &comindly::config::local_verifier());
    match comindly::answers::why(&g, &entries, focus) {
        Some(w) => render_why(&w),
        None => {
            comindly::ui::err(&format!("no symbol matches `{focus}`"));
            ExitCode::FAILURE
        }
    }
}

fn render_why(w: &comindly::answers::Why) -> ExitCode {
    println!("{}", w.markdown);
    ExitCode::SUCCESS
}

fn cmd_review(id: &str, action: &str, reason: &str, from: Option<&str>) -> ExitCode {
    let root = comindly::config::Config::load().index_dir(from);
    let records = comindly::ledger::all_blocking(&root).unwrap_or_default();
    let Some(target) = records.iter().find(|r| r.id == id || r.id.starts_with(id)) else {
        comindly::ui::err(&format!("no ledger record `{id}`"));
        return ExitCode::FAILURE;
    };
    let put = comindly::ledger::Record::review(target, action, reason, human()).and_then(|rec| {
        comindly::ledger::put_blocking(&root, &rec, comindly::config::local_signing_key())
    });
    match put {
        Ok(()) => {
            comindly::cache::invalidate(&root);
            comindly::ui::ok(&format!("{action}: `{}`", target.title));
            ExitCode::SUCCESS
        }
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn cmd_ledger(from: Option<&str>, inbox: bool, check: bool, compact: bool) -> ExitCode {
    let cfg = comindly::config::Config::load();
    if compact {
        return match comindly::ledger::compact_blocking(&cfg.index_dir(from)) {
            Ok((added, removed)) => {
                comindly::ui::ok(&format!(
                    "ledger compacted: {added} added, {removed} erased record(s) dropped"
                ));
                ExitCode::SUCCESS
            }
            Err(e) => {
                comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
                ExitCode::FAILURE
            }
        };
    }
    let (symbols, edges) =
        comindly::index::read_graph_blocking(&cfg.graph_dir(from)).unwrap_or_default();
    let records = match comindly::ledger::all_blocking(&cfg.index_dir(from)) {
        Ok(r) => r,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
    };
    let entries = comindly::ledger::derive(&records, &symbols, &comindly::config::local_verifier());
    let packages =
        comindly::index::read_packages_blocking(&cfg.graph_dir(from)).unwrap_or_default();
    render_ledger(&comindly::answers::ledger(
        entries, &symbols, &edges, &packages, inbox, check,
    ))
}

fn render_ledger(l: &comindly::answers::Ledger) -> ExitCode {
    if let Some(note) = &l.note {
        comindly::ui::ok(note);
    } else {
        print!("{}", l.markdown);
    }
    if l.failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_erase(ids: &[String], actor: Option<&str>, yes: bool, from: Option<&str>) -> ExitCode {
    if ids.is_empty() && actor.is_none() {
        comindly::ui::err("name record ids or --actor (`me` for your own records)");
        return ExitCode::FAILURE;
    }
    if let Some(short) = ids.iter().find(|i| i.len() < 8) {
        comindly::ui::err(&format!("id prefix `{short}` is too short (8+ characters)"));
        return ExitCode::FAILURE;
    }
    let cfg = comindly::config::Config::load();
    let all = comindly::ledger::all_blocking(&cfg.index_dir(from)).unwrap_or_default();
    for i in ids {
        let n = all.iter().filter(|r| r.id.starts_with(i.as_str())).count();
        if n != 1 {
            comindly::ui::err(&format!(
                "id prefix `{i}` matches {n} records; give a longer one"
            ));
            return ExitCode::FAILURE;
        }
    }
    let actor = actor.map(|a| {
        if a == "me" {
            comindly::config::local_identity().actor
        } else {
            a.to_string()
        }
    });
    let pick = |r: &comindly::ledger::Record| {
        ids.iter().any(|i| r.id.starts_with(i.as_str()))
            || actor.as_deref() == Some(r.provenance.actor.as_str())
    };
    match comindly::ledger::erase_blocking(&cfg.index_dir(from), pick, yes) {
        Ok(gone) => {
            for r in &gone {
                println!("- {} `{}` ({})", r.kind.as_str(), r.title, r.id);
            }
            if yes {
                comindly::ui::ok(&format!("erased {} record(s)", gone.len()));
            } else {
                comindly::ui::ok(&format!(
                    "{} record(s) would be erased; re-run with --yes",
                    gone.len()
                ));
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn cmd_ui(from: Option<&str>, web: Option<(String, u16)>, open: bool) -> ExitCode {
    use anyhow::Context as _;
    let root = comindly::config::Config::load().index_dir(from);
    let load = match web {
        Some(_) => Ok(comindly::view::Data::load_or_empty(
            &root,
            &comindly::config::local_verifier(),
        )),
        None => comindly::view::Data::load(&root, &comindly::config::local_verifier()),
    };
    let run = load.and_then(|data| match web {
        Some((host, port)) => {
            let addr = format!("{host}:{port}");
            let listener =
                std::net::TcpListener::bind(&addr).with_context(|| format!("bind {addr}"))?;
            if let Some(cfg) = comindly::server::ServerConfig::from_env()? {
                let (url, sign_in) = (cfg.public_url.clone(), cfg.sign_in);
                let server =
                    comindly::server::Server::open(&root, cfg).context("open the server table")?;
                migrate_server(&server, &root)?;
                comindly::ui::ok(&format!("comindly server for {url}, listening on {addr}"));
                comindly::ui::note(match sign_in {
                    comindly::server::SignIn::Email => "members sign in by email",
                    comindly::server::SignIn::Code => "members sign in with codes admins hand out",
                });
                return comindly::web::serve(
                    data,
                    listener,
                    comindly::web::Access::Server(Arc::new(server)),
                );
            }
            let token = comindly::config::ui_token()?;
            let url = format!("http://localhost:{port}/?token={token}");
            comindly::ui::ok(&format!("comindly web UI on {url} (Ctrl-C to stop)"));
            if open {
                open_in_browser(&url);
            }
            comindly::web::serve(data, listener, comindly::web::Access::Local(token))
        }
        None => comindly::tui::App::new(data).run(),
    });
    match run {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn cmd_repo_list(what: Option<ReposWhat>, from: Option<&str>) -> ExitCode {
    use comindly::config::RepoEntry;
    let cfg = comindly::config::Config::load();
    let mut list = cfg.repos.clone();
    let absolute = |p: &str| {
        std::fs::canonicalize(comindly::config::expand_tilde(p))
            .map(|c| c.to_string_lossy().into_owned())
            .unwrap_or_else(|_| p.to_string())
    };
    let changed = match what {
        None => {
            let shown = cmd_repos(from);
            comindly::ui::header(
                "Indexed by `comindly link` (default branch, comindly's own worktree)",
            );
            if list.is_empty() {
                comindly::ui::note("none yet: `comindly repos add <folder>`");
            }
            for r in &list {
                let p = Path::new(&r.path);
                let branch = comindly::worktree::default_branch(p).unwrap_or_else(|_| "?".into());
                comindly::ui::field(
                    &comindly::worktree::name(p),
                    &format!("{}  (indexes {branch})", r.path),
                );
            }
            let _ = shown;
            return ExitCode::SUCCESS;
        }
        Some(ReposWhat::Scan { dir, depth }) => {
            let listed: Vec<String> = list.iter().map(|r| absolute(&r.path)).collect();
            for p in
                comindly::worktree::scan(Path::new(&comindly::config::expand_tilde(&dir)), depth)
            {
                let abs = p.to_string_lossy().into_owned();
                let mark = if listed.contains(&abs) { "✓" } else { " " };
                println!("{mark} {abs}");
            }
            return ExitCode::SUCCESS;
        }
        Some(ReposWhat::Add { paths }) => {
            for p in paths {
                let entry = if p.contains("://") || p.starts_with("git@") {
                    match comindly::hosting::managed_path(
                        Path::new(&comindly::config::data_dir()),
                        &p,
                    ) {
                        Ok(dest) => RepoEntry {
                            path: dest.to_string_lossy().into_owned(),
                            remote: Some(p.clone()),
                        },
                        Err(e) => {
                            comindly::ui::err(&format!("{e:#}"));
                            return ExitCode::FAILURE;
                        }
                    }
                } else {
                    let path = absolute(&p);
                    if !Path::new(&path).join(".git").exists() {
                        comindly::ui::err(&format!("{p} is not a git repository"));
                        return ExitCode::FAILURE;
                    }
                    RepoEntry { path, remote: None }
                };
                if !list
                    .iter()
                    .any(|r| absolute(&r.path) == absolute(&entry.path))
                {
                    list.push(entry);
                }
            }
            true
        }
        Some(ReposWhat::Remove { repos }) => {
            let before = list.len();
            list.retain(|r| {
                let name = comindly::worktree::name(Path::new(&r.path));
                let gone = repos
                    .iter()
                    .any(|x| *x == name || absolute(x) == absolute(&r.path));
                if gone {
                    let _ = comindly::worktree::remove(
                        &comindly::config::cache_dir(),
                        Path::new(&r.path),
                    );
                }
                !gone
            });
            before != list.len()
        }
    };
    if changed {
        if let Err(e) = comindly::config::set_repos(&list) {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
        comindly::ui::ok(&format!(
            "{} repositories; index them with `comindly link`",
            list.len()
        ));
    }
    ExitCode::SUCCESS
}

/// Read a secret: the environment variable, stdin, or a hidden prompt.
fn read_secret(env: &str, stdin: bool, prompt: &str) -> anyhow::Result<String> {
    if let Ok(v) = std::env::var(env) {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    let v = if stdin {
        let mut s = String::new();
        std::io::stdin().read_line(&mut s)?;
        s
    } else {
        rpassword::prompt_password(prompt)?
    };
    let v = v.trim().to_string();
    anyhow::ensure!(!v.is_empty(), "nothing entered");
    Ok(v)
}

fn cmd_team(what: Option<TeamWhat>, from: Option<&str>) -> ExitCode {
    let root = comindly::config::Config::load().index_dir(from);
    match team_run(what, &root) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn team_run(what: Option<TeamWhat>, root: &str) -> anyhow::Result<()> {
    use anyhow::Context as _;
    use comindly::team;
    let stored = comindly::rt::block_on(team::load(root))?;
    let mut s = stored.settings.clone();
    match what {
        None => {
            comindly::ui::header(&format!("Team settings of {root}"));
            let show = |k: &str, v: Option<String>, default: &str| {
                let shown = v.unwrap_or_else(|| format!("{default}  (comindly default)"));
                comindly::ui::field(k, &shown)
            };
            show("name", s.name.clone(), "none");
            show(
                "llm_base_url",
                s.llm_base_url.clone(),
                comindly::llm::DEFAULT_BASE_URL,
            );
            show(
                "llm_model",
                s.llm_model.clone(),
                comindly::llm::DEFAULT_MODEL,
            );
            show(
                "embed_model",
                s.embed_model.clone(),
                comindly::embed::DEFAULT_MODEL,
            );
            for (k, v) in [
                ("embed", s.embed),
                ("enrich", s.enrich),
                ("flows", s.flows),
                ("guide", s.guide),
            ] {
                show(k, v.map(|b| b.to_string()), "true");
            }
            show(
                "max_enrich",
                s.max_enrich.map(|n| n.to_string()),
                "no limit",
            );
            show("max_flows", s.max_flows.map(|n| n.to_string()), "no limit");
            show(
                "llm_concurrency",
                s.llm_concurrency.map(|n| n.to_string()),
                "8",
            );
            show(
                "llm_price_in",
                s.llm_price_in.map(|n| n.to_string()),
                "not set",
            );
            show(
                "llm_price_out",
                s.llm_price_out.map(|n| n.to_string()),
                "not set",
            );
            show("format", s.format.clone(), "md");
            let source = match comindly::credentials::llm_key(s.name.as_deref(), None).1 {
                comindly::credentials::Source::Team => "your key for this team (credentials file)",
                comindly::credentials::Source::Default => "your default key (credentials file)",
                comindly::credentials::Source::Environment => "OPENAI_API_KEY",
                comindly::credentials::Source::Server => "stored on this server",
                comindly::credentials::Source::None => "none: `comindly team key`",
            };
            comindly::ui::field("api key on this machine", source);
            if let (Some(by), Some(at)) = (stored.changed_by, stored.changed_at) {
                comindly::ui::note(&format!(
                    "last changed by {by} at {}",
                    &at[..at.len().min(16)]
                ));
            }
            return Ok(());
        }
        Some(TeamWhat::Key { stdin, default }) => {
            let team = if default { None } else { s.name.clone() };
            let key = read_secret("COMINDLY_NEW_LLM_KEY", stdin, "LLM API key: ")?;
            comindly::credentials::set_llm_key(team.as_deref(), &key)?;
            let r#for = team.map_or("your default".to_string(), |t| format!("team {t}"));
            comindly::ui::ok(&format!(
                "key saved for {} in {}",
                r#for,
                comindly::credentials::path().display()
            ));
            return Ok(());
        }
        Some(TeamWhat::Set { pairs }) => {
            for p in pairs {
                let (k, v) = p
                    .split_once('=')
                    .with_context(|| format!("`{p}` is not KEY=VALUE"))?;
                let v = v.trim();
                let text = (!v.is_empty()).then(|| v.to_string());
                let number = |k: &str, v: &str| -> anyhow::Result<Option<usize>> {
                    Ok(if v.is_empty() {
                        None
                    } else {
                        Some(
                            v.parse()
                                .with_context(|| format!("{k} is a whole number"))?,
                        )
                    })
                };
                let price =
                    |k: &str, v: &str| -> anyhow::Result<Option<f64>> {
                        Ok(if v.is_empty() {
                            None
                        } else {
                            Some(v.parse().with_context(|| {
                                format!("{k} is a number, USD per million tokens")
                            })?)
                        })
                    };
                let flag = || -> anyhow::Result<Option<bool>> {
                    Ok(if v.is_empty() {
                        None
                    } else {
                        Some(v.parse().with_context(|| format!("{k} is true or false"))?)
                    })
                };
                match k.trim() {
                    "name" => s.name = text,
                    "llm_model" => s.llm_model = text,
                    "llm_base_url" => s.llm_base_url = text,
                    "embed_model" => {
                        comindly::ui::warn(
                            "a new embedding model needs a full re-index (`comindly link`)",
                        );
                        s.embed_model = text
                    }
                    "embed" => s.embed = flag()?,
                    "enrich" => s.enrich = flag()?,
                    "flows" => s.flows = flag()?,
                    "guide" => s.guide = flag()?,
                    "max_enrich" => s.max_enrich = number(k, v)?,
                    "max_flows" => s.max_flows = number(k, v)?,
                    "llm_concurrency" => {
                        s.llm_concurrency = number(k, v)?;
                        anyhow::ensure!(
                            s.llm_concurrency.is_none_or(|n| (1..=64).contains(&n)),
                            "llm_concurrency is between 1 and 64"
                        );
                    }
                    "llm_price_in" => s.llm_price_in = price(k, v)?,
                    "llm_price_out" => s.llm_price_out = price(k, v)?,
                    "format" => {
                        anyhow::ensure!(matches!(v, "" | "md" | "json"), "format is md or json");
                        s.format = text
                    }
                    "scip" => {
                        anyhow::ensure!(
                            v.is_empty() || comindly::scip::Mode::parse(v).is_some(),
                            "scip is auto, require or off"
                        );
                        s.scip = text
                    }
                    "auto_index" => s.auto_index = flag()?,
                    other => anyhow::bail!("unknown team setting `{other}`"),
                }
            }
        }
    }
    comindly::rt::block_on(comindly::config::save_team(root, &s, stored.version))?;
    comindly::ui::ok("team settings saved; everyone using this index gets them");
    Ok(())
}

fn cmd_join(index: &str) -> ExitCode {
    let root = comindly::config::expand_tilde(index.trim());
    let run = || -> anyhow::Result<()> {
        let graph = format!("{}/_graph", root.trim_end_matches('/'));
        let versions = comindly::rt::block_on(comindly::index::graph_versions(&graph))?;
        anyhow::ensure!(!versions.is_empty(), "no index at {root}");
        let stored = comindly::rt::block_on(comindly::team::load(&root))?;
        comindly::config::update(&[("index_dir", comindly::config::Setting::Text(root.clone()))])?;
        let name = stored.settings.name.clone();
        comindly::ui::ok(&format!("joined {}", name.as_deref().unwrap_or(&root)));
        if comindly::credentials::llm_key(name.as_deref(), None)
            .0
            .is_none()
        {
            comindly::ui::note(
                "for LLM steps, add your API key: `comindly team key` (or set OPENAI_API_KEY)",
            );
        }
        comindly::ui::note("connect your agent: `comindly app` → Connect, or `claude mcp add --scope user comindly -- comindly serve`");
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn cmd_app(from: Option<&str>) -> ExitCode {
    #[cfg(target_os = "macos")]
    {
        let root = comindly::config::Config::load().index_dir(from);
        match comindly::app::run(root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
                ExitCode::FAILURE
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = from;
        comindly::ui::err("the desktop app is macOS only for now; use `comindly ui --web --open`");
        ExitCode::FAILURE
    }
}

fn open_in_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}

fn cmd_preflight(repo: &str, base: &str, strict: bool, from: Option<&str>) -> ExitCode {
    let root = Path::new(repo);
    let name = std::fs::canonicalize(root)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| repo_name(repo));
    let raw = match comindly::git::working_changes(root, base) {
        Ok(c) => c,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
    };
    if raw.is_empty() {
        comindly::ui::ok("no changes against the base");
        return ExitCode::SUCCESS;
    }
    let cfg = comindly::config::Config::load();
    let (symbols, edges) = match comindly::index::read_graph_blocking(&cfg.graph_dir(from)) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&format!("no index: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let changes: Vec<comindly::preflight::Change> = raw
        .into_iter()
        .map(|(path, lines)| comindly::preflight::Change {
            repo: name.clone(),
            path,
            lines,
        })
        .collect();
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let records = comindly::ledger::all_blocking(&cfg.index_dir(from)).unwrap_or_default();
    let entries = comindly::ledger::derive(&records, &symbols, &comindly::config::local_verifier());
    let recipes = comindly::index::read_recipes_blocking(&cfg.graph_dir(from)).unwrap_or_default();
    let packages =
        comindly::index::read_packages_blocking(&cfg.graph_dir(from)).unwrap_or_default();
    let findings = comindly::checks::run(&entries, &symbols, &edges, &packages);
    println!(
        "{}",
        comindly::preflight::report(&g, &symbols, &edges, &entries, &recipes, &findings, &changes)
    );
    let changed: Vec<String> = changes
        .iter()
        .map(|c| format!("{}/{}", c.repo, c.path))
        .collect();
    let blocking = findings.iter().any(|f| {
        f.severity == "must"
            && f.violations
                .iter()
                .any(|v| changed.contains(&format!("{}/{}", v.repo, v.path)))
    });
    if strict && blocking {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Claude Code hook: emit `additionalContext` with ledger records relevant to the session/edit.
/// Never blocks or approves anything; any failure prints nothing and exits 0.
/// Graph symbols and ledger records for a hook: a remote (S3) index is read from its local
/// mirror, refreshed in the background when stale, so the agent never waits on the network.
fn hook_data(
    cfg: &comindly::config::Config,
    from: Option<&str>,
) -> Option<(Vec<Symbol>, Vec<comindly::ledger::Record>)> {
    let root = cfg.index_dir(from);
    if comindly::cache::is_remote(&root) {
        let dir = comindly::cache::dir(&root)?;
        let ttl = std::time::Duration::from_secs(
            cfg.cache_ttl_secs.unwrap_or(comindly::cache::DEFAULT_TTL),
        );
        if comindly::cache::stale(&dir, ttl) {
            comindly::cache::refresh_in_background(&root);
        }
        if let Some((symbols, _, records)) = comindly::cache::read(&dir) {
            return Some((symbols, records));
        }
    }
    let (symbols, _) = comindly::index::read_graph_blocking(&cfg.graph_dir(from)).ok()?;
    let records = comindly::ledger::all_blocking(&root).ok()?;
    Some((symbols, records))
}

/// The first string under `key` anywhere in a hook payload (agents nest tool input differently).
fn find_str<'v>(v: &'v serde_json::Value, key: &str) -> Option<&'v str> {
    match v {
        serde_json::Value::Object(m) => m
            .get(key)
            .and_then(|x| x.as_str())
            .or_else(|| m.values().find_map(|x| find_str(x, key))),
        serde_json::Value::Array(a) => a.iter().find_map(|x| find_str(x, key)),
        _ => None,
    }
}

/// An older server kept `config.toml` in its table: its repositories move to the server's
/// settings and its index settings to the index, once.
fn migrate_server(server: &comindly::server::Server, root: &str) -> anyhow::Result<()> {
    let Some(text) = server.settings.legacy_config() else {
        return Ok(());
    };
    if server.settings.repos().is_empty() {
        let old: comindly::config::Config = toml::from_str(&text).unwrap_or_default();
        server.settings.set_repos("upgrade", &old.repos)?;
    }
    comindly::rt::block_on(async {
        let stored = comindly::team::load(root).await?;
        let mut settings = stored.settings;
        if comindly::config::adopt_legacy(&text, &mut settings) {
            comindly::team::save_as(root, &settings, stored.version, "upgrade").await?;
        }
        anyhow::Ok(())
    })?;
    server.settings.drop_legacy_config()?;
    comindly::ui::note("moved the server's kept config.toml into its settings and the index's");
    Ok(())
}

/// The settings the index at `from` runs with on this machine.
fn settings_for(from: Option<&str>) -> comindly::context::Settings {
    let root = comindly::config::Config::load().index_dir(from);
    comindly::rt::block_on(comindly::config::index_settings(&root))
}

fn cmd_doctor(from: Option<&str>) -> ExitCode {
    let cfg = comindly::config::Config::load();
    let root = cfg.index_dir(from);
    comindly::ui::header("comindly doctor");
    comindly::ui::field("index", &root);
    let team = comindly::rt::block_on(comindly::team::load(&root))
        .map(|t| t.settings)
        .unwrap_or_default();
    let checks = comindly::doctor::run(&team, None, &root);
    for c in &checks {
        if c.ok {
            comindly::ui::ok(&c.message);
        } else {
            comindly::ui::warn(&c.message);
        }
    }
    if checks.iter().all(|c| c.ok) {
        comindly::ui::ok("all good");
    }
    ExitCode::SUCCESS
}

fn cmd_hook(agent: &str, event: &str, from: Option<&str>) -> ExitCode {
    use std::io::Read;
    let claude = agent == "claude";
    if !claude && !matches!(agent, "generic" | "cursor" | "gemini" | "codex") {
        comindly::ui::err("agent must be claude, cursor, gemini, codex or generic");
        return ExitCode::FAILURE;
    }
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let v: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let cwd = v
        .get("cwd")
        .and_then(|c| c.as_str())
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    let session = [
        "session_id",
        "sessionId",
        "conversation_id",
        "conversationId",
    ]
    .iter()
    .find_map(|k| find_str(&v, k))
    .unwrap_or("default")
    .to_string();
    let cfg = comindly::config::Config::load();
    let Some((symbols, records)) = hook_data(&cfg, from) else {
        return ExitCode::SUCCESS;
    };
    if records.is_empty() {
        return ExitCode::SUCCESS;
    }
    let entries = comindly::ledger::derive(&records, &symbols, &comindly::config::local_verifier());
    let (hook_event, ids): (&str, Vec<String>) = match event {
        "session-start" => {
            // Every symbol of the repo this session works in.
            let repo = repo_at(&symbols, &cwd);
            let ids = symbols
                .iter()
                .filter(|s| Some(&s.repo.0) == repo.as_ref())
                .map(|s| s.id.render())
                .collect();
            ("SessionStart", ids)
        }
        "pre-edit" => {
            let env = comindly::config::env("COMINDLY_HOOK_FILE");
            let Some(path) = env.as_deref().or_else(|| {
                ["file_path", "filePath", "target_file", "path", "file"]
                    .iter()
                    .find_map(|k| find_str(&v, k))
            }) else {
                return ExitCode::SUCCESS;
            };
            let ids = symbols
                .iter()
                .filter(|s| !s.file_path.is_empty() && path.ends_with(&format!("/{}", s.file_path)))
                .map(|s| s.id.render())
                .collect();
            ("PreToolUse", ids)
        }
        _ => return ExitCode::SUCCESS,
    };
    let seen_file = std::env::temp_dir()
        .join("comindly-hook-sessions")
        .join(session.replace(['/', '\\'], "_"));
    let seen: HashSet<String> = std::fs::read_to_string(&seen_file)
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let fresh: Vec<&comindly::ledger::Entry> = comindly::ledger::about(&entries, &ids)
        .into_iter()
        .filter(|e| e.trust >= comindly::ledger::Trust::T2)
        .filter(|e| {
            matches!(
                e.status,
                comindly::ledger::Status::Active
                    | comindly::ledger::Status::Moved
                    | comindly::ledger::Status::Stale
            )
        })
        .filter(|e| !seen.contains(&e.record.id))
        .take(if hook_event == "SessionStart" { 8 } else { 4 })
        .collect();
    if fresh.is_empty() {
        return ExitCode::SUCCESS;
    }
    let mut text = comindly::ledger::markdown(
        "comindly ledger: reasoning recorded about this code",
        &fresh,
    );
    if text.len() > 9_000 {
        let cut = (0..=9_000)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0);
        text.truncate(cut);
    }
    let _ = std::fs::create_dir_all(seen_file.parent().unwrap_or(std::path::Path::new(".")));
    let mut lines: Vec<String> = seen.into_iter().collect();
    lines.extend(fresh.iter().map(|e| e.record.id.clone()));
    let _ = std::fs::write(&seen_file, lines.join("\n"));
    if !claude {
        println!("{text}");
        return ExitCode::SUCCESS;
    }
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": hook_event,
            "additionalContext": text,
        }
    });
    println!("{out}");
    ExitCode::SUCCESS
}

/// The indexed repo a working directory belongs to (by directory name).
fn repo_at(symbols: &[Symbol], cwd: &Path) -> Option<String> {
    let repos: BTreeSet<&str> = symbols.iter().map(|s| s.repo.0.as_str()).collect();
    cwd.ancestors()
        .filter_map(|a| a.file_name().and_then(|n| n.to_str()))
        .find(|n| repos.contains(n))
        .map(str::to_string)
}

fn cmd_init_hooks(agent: &str) -> ExitCode {
    if agent != "claude" {
        comindly::ui::err("only `claude` is supported so far");
        return ExitCode::FAILURE;
    }
    let snippet = serde_json::json!({
        "hooks": {
            "SessionStart": [{
                "hooks": [{"type": "command", "command": "comindly hook claude session-start"}]
            }],
            "PreToolUse": [{
                "matcher": "Edit|MultiEdit|Write",
                "hooks": [{"type": "command", "command": "comindly hook claude pre-edit"}]
            }]
        }
    });
    comindly::ui::note("add to .claude/settings.json (project) or ~/.claude/settings.json (user):");
    println!(
        "{}",
        serde_json::to_string_pretty(&snippet).unwrap_or_default()
    );
    ExitCode::SUCCESS
}

/// Measured conventions with the team's changes applied, plus those changes.
fn load_conventions(
    from: Option<&str>,
) -> anyhow::Result<(
    Vec<comindly::conventions::Convention>,
    Vec<comindly::conventions::Manual>,
)> {
    let cfg = comindly::config::Config::load();
    let dir = cfg.graph_dir(from);
    let (symbols, edges) = comindly::index::read_graph_blocking(&dir)
        .map_err(|e| anyhow::anyhow!("no index at {dir}: {e:#}"))?;
    let Some(facts) = comindly::index::read_facts_blocking(&dir)? else {
        anyhow::bail!("index has no stored facts; re-run `comindly index`/`link`");
    };
    let mut all = comindly::conventions::mine(&symbols, &edges, &facts);
    all.extend(comindly::index::read_recipes_blocking(&dir).unwrap_or_default());
    let manual = comindly::conventions::manual_blocking(&cfg.index_dir(from))?;
    Ok((comindly::conventions::apply(all, &manual), manual))
}

fn cmd_conventions(from: Option<&str>, repo: Option<&str>) -> ExitCode {
    let (all, manual) = match load_conventions(from) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
    };
    render_conventions(&comindly::answers::conventions(all, &manual, repo))
}

fn render_conventions(c: &comindly::answers::Conventions) -> ExitCode {
    use comindly::conventions::{Origin, ORG};
    let mut by_repo: BTreeMap<&str, Vec<&comindly::conventions::Convention>> = BTreeMap::new();
    for c in &c.conventions {
        by_repo.entry(c.repo.as_str()).or_default().push(c);
    }
    for (r, cs) in by_repo {
        comindly::ui::header(if r == ORG { "Org baseline" } else { r });
        for c in cs {
            let tag = match c.origin {
                Origin::Measured => "",
                Origin::Edited => ", edited",
                Origin::Manual => ", team",
            };
            comindly::ui::field(&c.dimension, &format!("{}  [{}{tag}]", c.rule, c.id()));
            if !c.exemplars.is_empty() {
                comindly::ui::note(&format!("e.g. {}", c.exemplars.join(", ")));
            }
            if !c.counterexamples.is_empty() {
                comindly::ui::note(&format!("except {}", c.counterexamples.join(", ")));
            }
        }
    }
    if !c.hidden.is_empty() {
        comindly::ui::header("Hidden (comindly conventions restore <id>)");
        for m in &c.hidden {
            comindly::ui::field(&m.dimension, &format!("{}  [{}, {}]", m.rule, m.id, m.repo));
        }
    }
    ExitCode::SUCCESS
}

fn cmd_conventions_change(
    from: Option<&str>,
    repo: Option<&str>,
    what: ConventionsWhat,
) -> ExitCode {
    use comindly::conventions::{change, Change, Manual, Write, ORG};
    let root = comindly::config::Config::load().index_dir(from);
    let restore = matches!(what, ConventionsWhat::Restore { .. });
    let actor = comindly::config::local_identity().actor;
    let write = match what {
        ConventionsWhat::Add { rule, dimension } => Ok(Write::Put(Manual::new(
            repo.unwrap_or(ORG),
            &dimension,
            &rule,
            &actor,
        ))),
        ConventionsWhat::Edit { id, rule } => load_conventions(from)
            .and_then(|(all, manual)| change(&all, &manual, &id, Change::Edit(rule), &actor)),
        ConventionsWhat::Rm { id } => load_conventions(from)
            .and_then(|(all, manual)| change(&all, &manual, &id, Change::Hide, &actor)),
        ConventionsWhat::Restore { id } => load_conventions(from)
            .and_then(|(all, manual)| change(&all, &manual, &id, Change::Restore, &actor)),
    };
    match write.and_then(|w| comindly::conventions::write_blocking(&root, &w).map(|_| w)) {
        Ok(Write::Put(m)) if m.hidden => comindly::ui::ok(&format!("hid {}", m.id)),
        Ok(Write::Put(m)) => comindly::ui::ok(&format!("saved {}: {}", m.id, m.rule)),
        Ok(Write::Delete(id)) if restore => comindly::ui::ok(&format!("restored {id}")),
        Ok(Write::Delete(id)) => comindly::ui::ok(&format!("removed {id}")),
        Err(e) => {
            comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn cmd_packages(from: Option<&str>, skew: bool) -> ExitCode {
    let dir = comindly::config::Config::load().graph_dir(from);
    let rows = match comindly::index::read_packages_blocking(&dir) {
        Ok(r) => r,
        Err(e) => {
            comindly::ui::err(&format!("no index at {dir}: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let deps: Vec<_> = comindly::manifests::internal(&rows)
        .into_iter()
        .filter(|d| !skew || d.skewed())
        .collect();
    let mut table = comfy_table::Table::new();
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .set_header(vec![
            "package",
            "published by",
            "consumer",
            "requires",
            "locked",
        ]);
    let opt = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
    for d in &deps {
        let locked = match (&d.locked, d.skewed()) {
            (Some(l), true) => format!("{l}  ≠"),
            _ => opt(&d.locked),
        };
        table.add_row(vec![
            format!("{} ({})", d.name, d.ecosystem),
            format!("{} @ {}", d.publisher, opt(&d.published)),
            d.consumer.clone(),
            opt(&d.requirement),
            locked,
        ]);
    }
    comindly::ui::header(&format!("Internal dependencies — {}", deps.len()));
    println!("{table}");
    ExitCode::SUCCESS
}

fn cmd_contracts(from: Option<&str>, kind: Option<&str>, dangling: bool) -> ExitCode {
    let dir = comindly::config::Config::load().graph_dir(from);
    let (symbols, edges) = match comindly::index::read_graph_blocking(&dir) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&format!("no index at {dir}: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    render_contracts(&comindly::answers::contracts(&g, kind, dangling))
}

fn render_contracts(c: &comindly::answers::Contracts) -> ExitCode {
    let list = |ns: &[comindly::graph::Node]| {
        let set: BTreeSet<String> = ns
            .iter()
            .map(|n| {
                if n.repo == comindly::resolve::CONTRACTS {
                    n.name.clone()
                } else {
                    format!("{}:{}", n.repo, n.name)
                }
            })
            .collect();
        if set.is_empty() {
            "—".to_string()
        } else {
            set.into_iter().collect::<Vec<_>>().join("\n")
        }
    };
    let mut table = comfy_table::Table::new();
    table
        .load_preset(comfy_table::presets::UTF8_FULL)
        .set_header(vec!["contract", "provided by", "used by"]);
    for v in &c.views {
        table.add_row(vec![
            v.contract.name.clone(),
            list(&v.providers),
            list(&v.consumers),
        ]);
    }
    comindly::ui::header(&format!(
        "Contracts — {} shown of {}",
        c.views.len(),
        c.total
    ));
    println!("{table}");
    ExitCode::SUCCESS
}

fn cmd_eval_graph(fixture: &str, verbose: bool, scip: Option<&str>, repos: &[String]) -> ExitCode {
    let only: Vec<&str> = repos.iter().map(String::as_str).collect();
    let r = match comindly::eval::eval_with(
        Path::new(fixture),
        (!only.is_empty()).then_some(only.as_slice()),
        scip.map(Path::new),
    ) {
        Ok(r) => r,
        Err(e) => {
            comindly::ui::err(&format!("eval failed: {e:#}"));
            return ExitCode::FAILURE;
        }
    };
    let pct = |x: f64| format!("{:.0}%", x * 100.0);
    let mut kinds = comfy_table::Table::new();
    kinds
        .load_preset(comfy_table::presets::UTF8_FULL)
        .set_header(vec![
            "kind",
            "found",
            "missed",
            "spurious",
            "precision",
            "recall",
        ]);
    for (k, c) in r.by_kind.iter().chain([(&"total".to_string(), &r.total)]) {
        kinds.add_row(vec![
            k.clone(),
            c.tp.to_string(),
            c.fn_.to_string(),
            c.fp.to_string(),
            pct(c.precision()),
            pct(c.recall()),
        ]);
    }
    comindly::ui::header(&format!("Graph eval — {fixture}"));
    println!("{kinds}");
    let mut cases = comfy_table::Table::new();
    cases
        .load_preset(comfy_table::presets::UTF8_FULL)
        .set_header(vec!["case", "found / expected"]);
    for (case, (found, expected)) in &r.by_case {
        cases.add_row(vec![case.clone(), format!("{found} / {expected}")]);
    }
    println!("{cases}");
    if scip.is_some() {
        comindly::ui::note(&format!("references answered by SCIP: {}", r.scip_answers));
    }
    if verbose {
        comindly::ui::header("Missed");
        for g in &r.missed {
            comindly::ui::note(&format!("[{}] {} {} -> {}", g.case, g.kind, g.src, g.dst));
        }
        comindly::ui::header("Spurious");
        for (k, s, d) in &r.spurious {
            comindly::ui::note(&format!("{k} {s} -> {d}"));
        }
    }
    ExitCode::SUCCESS
}

fn cmd_login(url: &str) -> ExitCode {
    let url = url.trim().trim_end_matches('/').to_string();
    let host = std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "this machine".into());
    let client = format!("comindly CLI on {host}");
    let approved = comindly::rt::block_on(comindly::client::login(&url, &client, |link, code| {
        comindly::ui::step(&format!("approve this sign-in in your browser: {link}"));
        comindly::ui::field("code", code);
        comindly::ui::note(
            "check that the page shows the same code; waiting for approval (Ctrl-C to stop)",
        );
        if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
            open_in_browser(link);
        }
    }));
    let saved = approved.and_then(|a| {
        comindly::credentials::set_login(Some(comindly::credentials::Login {
            url: url.clone(),
            token: a.token,
            email: a.email.clone(),
        }))?;
        Ok(a.email)
    });
    match saved {
        Ok(email) => {
            comindly::ui::ok(&format!("signed in to {url} as {email}"));
            comindly::ui::note("find, search, why, ledger, flow, contracts, conventions, remember, review and link now ask the server; --local uses the index here");
            ExitCode::SUCCESS
        }
        Err(e) => {
            comindly::ui::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

fn cmd_logout() -> ExitCode {
    match comindly::credentials::login() {
        None => {
            comindly::ui::note("not signed in to a server");
            ExitCode::SUCCESS
        }
        Some(l) => match comindly::credentials::set_login(None) {
            Ok(()) => {
                comindly::ui::ok(&format!("signed out of {} on this machine", l.url));
                comindly::ui::note("the access token stays valid until you revoke it in the app (Settings → Your access)");
                ExitCode::SUCCESS
            }
            Err(e) => {
                comindly::ui::err(&format!("{e:#}"));
                ExitCode::FAILURE
            }
        },
    }
}

fn cmd_config(action: Option<ConfigAction>) -> ExitCode {
    use anyhow::Context as _;
    let run = || -> anyhow::Result<()> {
        match action.unwrap_or(ConfigAction::Show) {
            ConfigAction::Show => {
                let s = comindly::config::stored()?;
                print!("{}", comindly::config::export(&s.doc)?);
                if let (Some(v), Some(at)) = (s.version, s.changed_at) {
                    comindly::ui::note(&format!("version {v}, saved {at}"));
                }
            }
            ConfigAction::Set { pairs } => {
                let mut changes = Vec::new();
                for p in &pairs {
                    let (k, v) = p
                        .split_once('=')
                        .with_context(|| format!("`{p}` is not KEY=VALUE"))?;
                    let v = v.trim();
                    use comindly::config::Setting;
                    let value = match (k.trim(), v) {
                        (_, "") => Setting::Remove,
                        ("record_names", v) => {
                            Setting::Flag(v.parse().context("record_names is true or false")?)
                        }
                        ("cache_ttl_secs", v) => Setting::Number(
                            v.parse::<i64>()
                                .context("cache_ttl_secs is a number of seconds")?
                                .max(30),
                        ),
                        ("index_dir", v) => Setting::Text(comindly::config::expand_tilde(v)),
                        (_, v) => Setting::Text(v.to_string()),
                    };
                    changes.push((k.trim(), value));
                }
                comindly::config::update(&changes)?;
                comindly::ui::ok("saved");
            }
            ConfigAction::Import { file, replace } => {
                let text =
                    std::fs::read_to_string(&file).with_context(|| format!("read {file}"))?;
                let r = comindly::config::import(&text, replace)?;
                comindly::ui::ok(if r.changed {
                    "imported"
                } else {
                    "nothing to change"
                });
                if !r.kept.is_empty() {
                    comindly::ui::note(&format!(
                        "kept your current {}; --override replaces them with the file's",
                        r.kept.join(", ")
                    ));
                }
                if r.moved {
                    comindly::ui::note(
                        "its models, LLM steps and limits went to the index's shared settings (`comindly team`)",
                    );
                }
            }
            ConfigAction::Export { file } => {
                let text = comindly::config::export(&comindly::config::stored()?.doc)?;
                match file {
                    Some(f) => {
                        std::fs::write(&f, text).with_context(|| format!("write {f}"))?;
                        comindly::ui::ok(&format!("wrote {f}"));
                    }
                    None => print!("{text}"),
                }
            }
            ConfigAction::History => {
                for v in comindly::config::history()? {
                    comindly::ui::field(
                        &format!("v{}", v.version),
                        &format!("{} by {}", v.changed_at, v.changed_by),
                    );
                }
            }
            ConfigAction::Path => {
                // Plain on stdout so `$(comindly config path)` is scriptable.
                println!("{}", comindly::config::store_path().display());
                comindly::ui::field(
                    "index dir",
                    &comindly::config::Config::load().index_dir(None),
                );
            }
        }
        Ok(())
    };
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            comindly::ui::err(&format!("{e:#}"));
            ExitCode::FAILURE
        }
    }
}

fn repo_name(path: &str) -> String {
    let p = Path::new(path);
    // Use the directory's own name; for `.`/`..`/relative paths, resolve to the real dir first.
    match p.file_name().and_then(|n| n.to_str()) {
        Some(n) if n != "." && n != ".." => n.to_string(),
        _ => std::fs::canonicalize(p)
            .ok()
            .and_then(|c| c.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "repo".to_string()),
    }
}

fn cmd_flow(focus: &str, from: Option<&str>) -> ExitCode {
    let uri = comindly::config::Config::load().graph_dir(from);
    let uri = uri.as_str();
    let (symbols, edges) = match comindly::index::read_graph_blocking(uri) {
        Ok(x) => x,
        Err(e) => {
            comindly::ui::err(&friendly_load_err(uri, &e));
            return ExitCode::FAILURE;
        }
    };
    let g = comindly::graph::CodeGraph::build(&symbols, &edges);
    let stored = |fid: &str| {
        comindly::index::read_flows_blocking(uri)
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .find(|(id, _, _)| id.render() == fid)
            .map(|(_, narr, queries)| (narr, queries))
    };
    match comindly::answers::flow(&g, focus, stored) {
        Some(f) => render_flow(&f),
        None => {
            comindly::ui::err(&format!("no symbol matching `{focus}`"));
            ExitCode::FAILURE
        }
    }
}

fn render_flow(f: &comindly::answers::Flow) -> ExitCode {
    let focus = &f.focus;
    match &f.narration {
        Some((narr, queries)) => {
            comindly::ui::header(&format!("flow: {focus}"));
            println!("{narr}\n");
            if !queries.is_empty() {
                comindly::ui::field("Ask", &queries.join(" · "));
            }
        }
        None => comindly::ui::note(&format!(
            "no pre-generated narration for `{focus}` — showing the raw trace; run `index/link --flows` to narrate"
        )),
    }
    comindly::ui::header(&format!(
        "flow trace from `{focus}` ({} steps)",
        f.steps.len()
    ));
    if f.steps.is_empty() {
        comindly::ui::note("no outgoing calls from here");
    } else {
        let mut t = comindly::ui::table(&["depth", "via", "symbol", "location"]);
        for h in &f.steps {
            t.add_row(vec![
                format!("d{}", h.depth),
                h.via.clone(),
                h.name.clone(),
                h.location.clone(),
            ]);
        }
        comindly::ui::right_align(&mut t, &[0]);
        println!("{t}");
    }
    ExitCode::SUCCESS
}

/// `n word` / `n words` — naive English pluralization for count phrases.
fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("{n} {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Turn a LanceDB "dataset/table not found" error into an actionable one-liner, hiding the raw
/// internal backtrace. `uri` is the internal `<root>/_graph` path; we show the root the user gave.
/// Which optional steps a command runs, after its flags.
struct Steps {
    embed: bool,
    enrich: bool,
    flows: bool,
    guide: bool,
}

/// Run an indexing command: one runtime, the resolved settings, the terminal renderer, Ctrl-C to
/// cancel. `None` (after a message) when it failed or was cancelled.
fn run_indexing<T, F, Fut>(root: &str, steps: Steps, job: F) -> Option<T>
where
    F: FnOnce(comindly::context::IndexContext) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let rt = comindly::rt::shared();
    rt.block_on(async {
        let team = comindly::team::load(root)
            .await
            .map(|s| s.settings)
            .unwrap_or_default();
        let mut settings = comindly::context::Settings::resolve(&team, comindly::config::env);
        settings.embed = steps.embed;
        settings.enrich = steps.enrich;
        settings.flows = steps.flows;
        settings.guide = steps.guide;
        let wants_llm = steps.enrich || steps.flows || steps.guide;
        let llm = if wants_llm {
            comindly::config::llm_client(&settings, team.name.as_deref(), None)
        } else {
            None
        };
        let ctx = comindly::context::IndexContext::new(
            root,
            settings,
            Arc::new(comindly::ui::Terminal::new()),
            comindly::config::cache_dir(),
        )
        .with_llm(llm)
        .with_git(comindly::config::git_credentials(None));
        let cancel = ctx.cancel.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                cancel.cancel();
            }
        });
        match job(ctx).await {
            Ok(v) => Some(v),
            Err(e) if e.is::<comindly::context::Cancelled>() => {
                comindly::ui::warn("cancelled; what was already saved stays");
                None
            }
            Err(e) => {
                comindly::ui::err(&comindly::storage::explain(&format!("{e:#}")));
                None
            }
        }
    })
}

/// Parse several repos (or the configured ones), bind references across them, persist the
/// graph, and report the most depended-on symbols.
#[allow(clippy::too_many_arguments)] // CLI dispatch: flags map 1:1 to clap args
fn cmd_link(
    repos: &[String],
    to: Option<&str>,
    embed: bool,
    enrich: bool,
    flows: bool,
    guide: bool,
    incremental: bool,
    scip: comindly::scip::Mode,
) -> ExitCode {
    let cfg = comindly::config::Config::load();
    let root = cfg.index_dir(to);
    let steps = Steps {
        embed,
        enrich,
        flows,
        guide,
    };
    let report = run_indexing(&root, steps, |ctx| async move {
        let sources = if repos.is_empty() {
            comindly::pipeline::checkout(&ctx, &comindly::config::expanded(&cfg.repos)).await?
        } else {
            repos.iter().map(comindly::pipeline::Source::at).collect()
        };
        comindly::pipeline::link(&ctx, sources, incremental, scip).await
    });
    let Some(report) = report else {
        return ExitCode::FAILURE;
    };
    if !report.dependencies.is_empty() {
        comindly::ui::header("Top cross-repo dependencies (org-wide blast radius)");
        let mut table = comindly::ui::table(&["symbol", "defined-in", "repos", "refs"]);
        for d in report.dependencies.iter().take(15) {
            table.add_row(vec![
                truncate_start(&d.descriptor, 46),
                d.defined_in.clone(),
                d.repos.len().to_string(),
                d.refs.to_string(),
            ]);
        }
        println!("{table}");
    }
    if let Some(d) = report.dependencies.first() {
        comindly::ui::header(&format!(
            "ripple({})  [defined in {}]",
            d.descriptor, d.defined_in
        ));
        comindly::ui::note(&format!(
            "changing this would impact {} repos ({} references):",
            d.repos.len(),
            d.refs
        ));
        for repo in &d.repos {
            comindly::ui::note(&format!("- {repo}"));
        }
    }
    ExitCode::SUCCESS
}

/// The last `n` characters, with a leading ellipsis when cut.
fn truncate_start(s: &str, n: usize) -> String {
    let len = s.chars().count();
    if len <= n {
        return s.to_string();
    }
    let tail: String = s.chars().skip(len - (n - 1)).collect();
    format!("…{tail}")
}

#[allow(clippy::too_many_arguments)] // CLI dispatch: flags map 1:1 to clap args
fn cmd_index(
    repo: &str,
    to: Option<&str>,
    embed: bool,
    enrich: bool,
    flows: bool,
    guide: bool,
    incremental: bool,
    since: Option<&str>,
    scip: comindly::scip::Mode,
) -> ExitCode {
    let root = comindly::config::Config::load().index_dir(to);
    let source = comindly::pipeline::Source::at(repo);
    let since = since.map(str::to_string);
    let ok = run_indexing(
        &root,
        Steps {
            embed,
            enrich,
            flows,
            guide,
        },
        |ctx| async move {
            comindly::pipeline::index_repo(&ctx, source, incremental, since, scip).await
        },
    );
    if ok.is_none() {
        return ExitCode::FAILURE;
    }
    if embed {
        comindly::ui::note("next: comindly search \"<question>\"  ·  comindly serve");
    } else {
        comindly::ui::note(
            "next: comindly explore <symbol> (search needs embeddings — drop --no-embed)",
        );
    }
    ExitCode::SUCCESS
}

fn friendly_load_err(uri: &str, e: &anyhow::Error) -> String {
    let root = uri.trim_end_matches("/_graph");
    if e.to_string().contains("not found") {
        format!("no index at {root} — build one first: comindly index <repo>")
    } else {
        format!("load from {root} failed: {e:#}")
    }
}
