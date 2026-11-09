//! Comindly LLM enrichment — per-symbol summaries, symbol-aware query generation, and a
//! repo style guide, via **Rig** (provider-agnostic: OpenAI by default; any OpenAI-compatible
//! endpoint via `COMINDLY_LLM_BASE_URL`; Bedrock/Vertex/others can be added behind cargo features).
//!
//! **Opt-in / data egress:** these functions send code (signatures, snippets) to the configured
//! LLM provider. They run only when the caller explicitly enables enrichment
//! (`comindly link --enrich`), never as part of plain indexing. Requires `OPENAI_API_KEY` (or a
//! provider key + `COMINDLY_LLM_BASE_URL`).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use rig::completion::CompletionRequest;
use rig::providers::openai;
use rig::schemars::{self, JsonSchema};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::prompts::{Prompts, Task};

/// Default model — cheap and fast, suitable for bulk symbol summaries.
pub const DEFAULT_MODEL: &str = "gpt-6-luna";

/// Where requests go without an endpoint setting: OpenAI.
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct LlmClient {
    // Chat Completions API (works with OpenAI *and* any OpenAI-compatible endpoint), not the
    // Responses API — proxies/Ollama/vLLM implement `/chat/completions`, not `/responses`.
    client: openai::OpenAI,
    model: String,
    /// Tasks that run on another model than `model`.
    models: std::collections::BTreeMap<Task, String>,
    // OpenAI's own API takes `max_completion_tokens` (required by newer models); compatible
    // servers (Ollama, vLLM, LiteLLM) expect `max_tokens`.
    openai_native: bool,
    // Cumulative token usage across all calls (for live progress + a final total).
    input_tokens: Arc<AtomicU64>,
    output_tokens: Arc<AtomicU64>,
}

/// LLM-generated enrichment for one symbol.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[schemars(crate = "rig::schemars")]
pub struct Enrichment {
    /// One sentence, at most 30 words, on what the code does, what for, and its effects.
    pub summary: String,
    /// Three questions someone would type to find this code, in the domain's words.
    pub queries: Vec<String>,
}

impl LlmClient {
    /// A client for `model` at `base_url` (OpenAI when absent), with `key`.
    pub fn new(base_url: Option<&str>, model: &str, key: Option<&str>) -> Result<Self> {
        let key = key.map(str::to_string);
        let client = match base_url {
            Some(base) => openai::OpenAIConfig::new(key.unwrap_or_else(|| "sk-noauth".into()))
                .with_base_url(base)
                .client(),
            None => openai::OpenAIConfig::new(key.context("no LLM key")?).client(),
        };
        Ok(Self {
            client,
            model: model.to_string(),
            models: Default::default(),
            openai_native: base_url.is_none(),
            input_tokens: Arc::new(AtomicU64::new(0)),
            output_tokens: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Cumulative `(input, output)` tokens used so far, across all calls.
    pub fn token_usage(&self) -> (u64, u64) {
        (
            self.input_tokens.load(Ordering::Relaxed),
            self.output_tokens.load(Ordering::Relaxed),
        )
    }

    /// Run `task` on its own model instead of the default one.
    pub fn with_models(mut self, models: std::collections::BTreeMap<Task, String>) -> Self {
        self.models = models;
        self
    }

    /// The model `task` runs on.
    pub fn model(&self, task: Task) -> &str {
        self.models.get(&task).unwrap_or(&self.model)
    }

    /// A reply in the shape of `T`, asked for as structured output (a JSON schema).
    async fn complete<T: DeserializeOwned + JsonSchema>(
        &self,
        task: Task,
        system: &str,
        user: &str,
        max_tokens: u32,
    ) -> Result<T> {
        // Chat Completions (not the Responses API): what OpenAI-compatible servers implement.
        let req = CompletionRequest::new(user.to_string())
            .preamble(system)
            .output_schema(schemars::schema_for!(T));
        // Reasoning models spend part of the completion budget thinking, hence the headroom.
        let req = if self.openai_native {
            req.additional_params(serde_json::json!({
                "max_completion_tokens": (max_tokens as u64 * 4).max(4000)
            }))
        } else {
            req.max_tokens(max_tokens as u64)
        };
        let resp = self
            .client
            .chat(self.model(task))
            .call(req)
            .await
            .context("LLM completion")?;
        self.input_tokens
            .fetch_add(resp.usage.input_tokens.unwrap_or(0), Ordering::Relaxed);
        self.output_tokens
            .fetch_add(resp.usage.output_tokens.unwrap_or(0), Ordering::Relaxed);
        parse(&resp.text())
    }

    /// One-line summary of what a symbol does, plus a few natural-language queries it answers.
    pub async fn enrich_symbol(&self, prompts: &Prompts, brief: &str) -> Result<Enrichment> {
        let r: Enrichment = self
            .complete(Task::Summary, &prompts.system(Task::Summary), brief, 400)
            .await?;
        Ok(Enrichment {
            summary: r.summary.trim().to_string(),
            queries: tidy(r.queries),
        })
    }

    /// ONE section of a repo's style guide from its tailored evidence, as bullet rules without a
    /// heading, or `None` when the evidence shows no consistent convention.
    pub async fn guide_section(
        &self,
        prompts: &Prompts,
        repo: &str,
        title: &str,
        guidance: &str,
        evidence: &str,
    ) -> Result<Option<String>> {
        let user = format!(
            "Repository: {repo}\nSection: {title}\nWhat to cover: {guidance}\n\n# Evidence\n\n{evidence}"
        );
        let r: GuideSection = self
            .complete(Task::Guide, &prompts.system(Task::Guide), &user, 900)
            .await?;
        let rules = tidy(r.rules);
        Ok((r.observed && !rules.is_empty()).then(|| {
            rules
                .iter()
                .map(|r| format!("- {}", r.trim_start_matches("- ")))
                .collect::<Vec<_>>()
                .join("\n")
        }))
    }

    /// Narrate an execution flow: given an entry point and its ordered call trace, produce a
    /// concise markdown walkthrough plus a few flow-oriented questions the flow answers.
    pub async fn narrate_flow(
        &self,
        prompts: &Prompts,
        entry: &str,
        signature: &str,
        trace: &str,
    ) -> Result<(String, Vec<String>)> {
        let user = format!("Entry point: {entry}\nSignature: {signature}\nCall trace:\n{trace}");
        let r: Walkthrough = self
            .complete(Task::Flow, &prompts.system(Task::Flow), &user, 700)
            .await?;
        Ok((r.narration.trim().to_string(), tidy(r.queries)))
    }
}

/// What a guide section call returns.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rig::schemars")]
struct GuideSection {
    /// False when the evidence shows no consistent convention.
    observed: bool,
    /// One rule per item, in markdown, without a leading dash.
    rules: Vec<String>,
}

/// What a flow call returns.
#[derive(Deserialize, JsonSchema)]
#[schemars(crate = "rig::schemars")]
struct Walkthrough {
    /// The walkthrough in 3 to 6 short sentences, at most 120 words, symbols in backticks.
    narration: String,
    /// Three questions this flow answers, in the domain's words.
    queries: Vec<String>,
}

/// The JSON schema of `task`'s reply, as text for the system message.
pub fn reply_schema(task: Task) -> String {
    let schema = match task {
        Task::Summary => schemars::schema_for!(Enrichment),
        Task::Guide => schemars::schema_for!(GuideSection),
        Task::Flow => schemars::schema_for!(Walkthrough),
    };
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

fn tidy(items: Vec<String>) -> Vec<String> {
    items
        .into_iter()
        .map(|q| q.trim().to_string())
        .filter(|q| !q.is_empty())
        .collect()
}

/// The JSON reply, also when a server that ignores the schema wraps it in prose or a code fence.
fn parse<T: DeserializeOwned>(raw: &str) -> Result<T> {
    let raw = raw.trim();
    if let Ok(v) = serde_json::from_str(raw) {
        return Ok(v);
    }
    let (Some(a), Some(b)) = (raw.find('{'), raw.rfind('}')) else {
        anyhow::bail!("the LLM did not reply with JSON");
    };
    anyhow::ensure!(a < b, "the LLM did not reply with JSON");
    serde_json::from_str(&raw[a..=b]).context("the LLM's JSON reply has the wrong shape")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_json_also_inside_prose() {
        let e: Enrichment =
            parse(r#"{"summary":"Opens a pooled connection.","queries":["how to connect"]}"#)
                .unwrap();
        assert_eq!(e.summary, "Opens a pooled connection.");
        let fenced =
            "Here it is:\n```json\n{\"narration\": \"Calls a, then b.\", \"queries\": []}\n```";
        let w: Walkthrough = parse(fenced).unwrap();
        assert_eq!(w.narration, "Calls a, then b.");
        assert!(parse::<Enrichment>("SUMMARY: no json").is_err());
        assert!(parse::<Enrichment>(r#"{"summary": 3}"#).is_err());
    }

    #[test]
    fn the_schema_is_strict_enough_for_openai() {
        let s = serde_json::to_value(schemars::schema_for!(GuideSection)).unwrap();
        assert_eq!(s["required"], serde_json::json!(["observed", "rules"]));
    }

    // Real API call — run with a key: `cargo test -p comindly-llm -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn live_enrich_symbol() {
        let key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY");
        let c = LlmClient::new(None, DEFAULT_MODEL, Some(&key)).unwrap();
        let e = c
            .enrich_symbol(
                &Prompts::default(),
                "Symbol: AsyncTaskRunner\nKind: Class in repository acme, acme/database/executors.py:12\nSignature: class AsyncTaskRunner(BaseTaskRunner)",
            )
            .await
            .expect("enrich");
        assert!(!e.summary.is_empty());
        eprintln!("summary: {}", e.summary);
        eprintln!("queries: {:?}", e.queries);
    }

    // A compatible server: `COMINDLY_TEST_LLM_BASE_URL` and `COMINDLY_TEST_LLM_MODEL`, e.g. Ollama.
    #[tokio::test]
    #[ignore]
    async fn live_compatible_server() {
        let base = std::env::var("COMINDLY_TEST_LLM_BASE_URL").expect("COMINDLY_TEST_LLM_BASE_URL");
        let model = std::env::var("COMINDLY_TEST_LLM_MODEL").expect("COMINDLY_TEST_LLM_MODEL");
        let c = LlmClient::new(Some(&base), &model, None).unwrap();
        let p = Prompts::default();
        let e = c
            .enrich_symbol(
                &p,
                "Symbol: open_pool\nKind: Function in repository acme, src/db/pool.rs:3\nSignature: fn open_pool(cfg: &DbConfig) -> Pool",
            )
            .await
            .expect("summary");
        assert!(!e.summary.is_empty());
        eprintln!("summary: {}\nqueries: {:?}", e.summary, e.queries);
        let (n, _) = c
            .narrate_flow(
                &p,
                "handle_order",
                "fn handle_order(o: Order)",
                "d1 calls acme:validate src/a.rs:3",
            )
            .await
            .expect("flow");
        eprintln!("flow: {n}");
    }

    #[tokio::test]
    #[ignore]
    async fn live_guide_and_flow() {
        let key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY");
        let c = LlmClient::new(None, DEFAULT_MODEL, Some(&key)).unwrap();
        let p = Prompts::default();
        let evidence = "snake_case functions 97% (1234/1268)\nsrc/db/pool.rs: pub fn open_pool(cfg: &DbConfig) -> Pool";
        let g = c
            .guide_section(&p, "acme", "Naming", "How things are named", evidence)
            .await
            .expect("guide");
        eprintln!("guide: {g:?}");
        let (n, q) = c
            .narrate_flow(
                &p,
                "handle_order",
                "fn handle_order(o: Order)",
                "d1 calls acme:validate src/a.rs:3\nd2 calls acme:save src/b.rs:9",
            )
            .await
            .expect("flow");
        assert!(!n.is_empty());
        eprintln!("flow: {n}\n{q:?}");
    }
}
