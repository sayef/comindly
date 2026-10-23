//! What a call into core needs, passed in rather than looked up: where the index lives, the
//! settings it runs with, the LLM, where events go, and when to stop.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::events::{EventSink, Reporter};
use crate::llm::LlmClient;
use crate::team::TeamSettings;

/// Settings resolved once at the edge: shared settings first, then the environment, then
/// `config.toml`, then comindly's defaults.
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
}

impl Settings {
    pub fn resolve(team: &TeamSettings, cfg: &Config) -> Settings {
        let env = |k: &str| crate::config::env(k);
        Settings {
            llm_model: team
                .llm_model
                .clone()
                .or_else(|| env("COMINDLY_LLM_MODEL"))
                .or_else(|| cfg.llm_model.clone())
                .unwrap_or_else(|| crate::llm::DEFAULT_MODEL.to_string()),
            llm_base_url: team
                .llm_base_url
                .clone()
                .or_else(|| env("COMINDLY_LLM_BASE_URL"))
                .or_else(|| cfg.llm_base_url.clone()),
            embed_model: team
                .embed_model
                .clone()
                .or_else(|| env("COMINDLY_EMBED_MODEL"))
                .or_else(|| cfg.embed_model.clone())
                .unwrap_or_else(|| crate::embed::DEFAULT_MODEL.to_string()),
            embed: team.embed.or(cfg.embed).unwrap_or(true),
            enrich: team.enrich.or(cfg.enrich).unwrap_or(true),
            flows: team.flows.or(cfg.flows).unwrap_or(true),
            guide: team.guide.or(cfg.guide).unwrap_or(true),
            max_enrich: team.max_enrich.or(cfg.max_enrich).unwrap_or(usize::MAX),
            max_flows: team.max_flows.or(cfg.max_flows).unwrap_or(usize::MAX),
            llm_concurrency: team.llm_concurrency.unwrap_or(8).clamp(1, 64),
            prices: (team.llm_price_in.is_some() || team.llm_price_out.is_some()).then(|| {
                (
                    team.llm_price_in.unwrap_or(0.0),
                    team.llm_price_out.unwrap_or(0.0),
                )
            }),
            format: env("COMINDLY_FORMAT")
                .or_else(|| team.format.clone())
                .or_else(|| cfg.format.clone())
                .unwrap_or_else(|| "md".to_string()),
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
    pub report: Reporter,
    pub cancel: CancellationToken,
}

impl IndexContext {
    pub fn new(root: impl Into<String>, settings: Settings, events: Arc<dyn EventSink>) -> Self {
        IndexContext {
            root: root.into(),
            settings,
            llm: None,
            report: Reporter::new(events),
            cancel: CancellationToken::new(),
        }
    }

    pub fn with_llm(mut self, llm: Option<LlmClient>) -> Self {
        self.llm = llm.map(Arc::new);
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
