//! What a call into core needs, passed in rather than looked up: where the index lives, the
//! settings it runs with, the LLM, where events go, and when to stop.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::events::{EventSink, Reporter};
use crate::llm::LlmClient;
use crate::team::TeamSettings;
use crate::worktree::{Credentials, NoCredentials};

/// Settings resolved once at the edge: the index's shared settings, then the environment, then
/// comindly's defaults.
#[derive(Debug, Clone)]
pub struct Settings {
    pub llm_model: String,
    pub llm_base_url: Option<String>,
    pub embed_model: String,
    pub embed: bool,
    pub enrich: bool,
    pub flows: bool,
    pub guide: bool,
    pub max_enrich: usize,
    pub max_flows: usize,
    pub llm_concurrency: usize,
    /// USD per million `(input, output)` tokens.
    pub prices: Option<(f64, f64)>,
    /// What MCP tools return as text: `md` or `json`.
    pub format: String,
    pub scip: crate::scip::Mode,
    pub auto_index: bool,
}

impl Settings {
    /// `env` looks up overrides such as `COMINDLY_LLM_MODEL`.
    pub fn resolve(team: &TeamSettings, env: impl Fn(&str) -> Option<String>) -> Settings {
        let env = |k: &str| env(k).filter(|v| !v.is_empty());
        Settings {
            llm_model: team
                .llm_model
                .clone()
                .or_else(|| env("COMINDLY_LLM_MODEL"))
                .unwrap_or_else(|| crate::llm::DEFAULT_MODEL.to_string()),
            llm_base_url: team
                .llm_base_url
                .clone()
                .or_else(|| env("COMINDLY_LLM_BASE_URL")),
            embed_model: team
                .embed_model
                .clone()
                .or_else(|| env("COMINDLY_EMBED_MODEL"))
                .unwrap_or_else(|| crate::embed::DEFAULT_MODEL.to_string()),
            embed: team.embed.unwrap_or(true),
            enrich: team.enrich.unwrap_or(true),
            flows: team.flows.unwrap_or(true),
            guide: team.guide.unwrap_or(true),
            max_enrich: team.max_enrich.unwrap_or(usize::MAX),
            max_flows: team.max_flows.unwrap_or(usize::MAX),
            llm_concurrency: team.llm_concurrency.unwrap_or(8).clamp(1, 64),
            prices: (team.llm_price_in.is_some() || team.llm_price_out.is_some()).then(|| {
                (
                    team.llm_price_in.unwrap_or(0.0),
                    team.llm_price_out.unwrap_or(0.0),
                )
            }),
            format: env("COMINDLY_FORMAT")
                .or_else(|| team.format.clone())
                .unwrap_or_else(|| "md".to_string()),
            scip: env("COMINDLY_SCIP")
                .or_else(|| team.scip.clone())
                .and_then(|s| crate::scip::Mode::parse(&s))
                .unwrap_or(crate::scip::Mode::Auto),
            auto_index: team.auto_index.unwrap_or(false),
        }
    }
}

/// Everything an indexing or query call needs.
#[derive(Clone)]
pub struct IndexContext {
    /// The index location: a folder, `s3://`, `gs://` or `az://`.
    pub root: String,
    pub settings: Settings,
    /// Absent: the LLM steps are skipped, and say so.
    pub llm: Option<Arc<LlmClient>>,
    /// Tokens for cloning and fetching over HTTPS.
    pub git: Arc<dyn Credentials>,
    pub report: Reporter,
    pub cancel: CancellationToken,
    /// Where comindly keeps its checkouts of each repository's default branch.
    pub cache: std::path::PathBuf,
}

impl IndexContext {
    pub fn new(
        root: impl Into<String>,
        settings: Settings,
        events: Arc<dyn EventSink>,
        cache: std::path::PathBuf,
    ) -> Self {
        IndexContext {
            root: root.into(),
            settings,
            llm: None,
            git: Arc::new(NoCredentials),
            report: Reporter::new(events),
            cancel: CancellationToken::new(),
            cache,
        }
    }

    pub fn with_llm(mut self, llm: Option<LlmClient>) -> Self {
        self.llm = llm.map(Arc::new);
        self
    }

    pub fn with_git(mut self, git: Arc<dyn Credentials>) -> Self {
        self.git = git;
        self
    }

    /// The graph tables: `<root>/_graph`.
    pub fn graph(&self) -> String {
        format!("{}/_graph", self.root.trim_end_matches('/'))
    }

    /// An error when the caller cancelled.
    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.cancel.is_cancelled(), Cancelled);
        Ok(())
    }
}

/// The work was cancelled; what was saved before stays.
#[derive(Debug, Clone, Copy)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cancelled")
    }
}

impl std::error::Error for Cancelled {}
