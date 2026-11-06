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
    // OpenAI's own API takes `max_completion_tokens` (required by newer models); compatible
    // servers (Ollama, vLLM, LiteLLM) expect `max_tokens`.
    openai_native: bool,
    // Cumulative token usage across all calls (for live progress + a final total).
    input_tokens: Arc<AtomicU64>,
    output_tokens: Arc<AtomicU64>,
}

/// LLM-generated enrichment for one symbol.
#[derive(Debug, Clone)]
pub struct Enrichment {
    pub summary: String,
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

    async fn complete(&self, system: &str, user: &str, max_tokens: u32) -> Result<String> {
        // Chat Completions (not the Responses API): what OpenAI-compatible servers implement.
        let req = CompletionRequest::new(user.to_string()).preamble(system);
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
            .chat(&self.model)
            .call(req)
            .await
            .context("LLM completion")?;
        self.input_tokens
            .fetch_add(resp.usage.input_tokens.unwrap_or(0), Ordering::Relaxed);
        self.output_tokens
            .fetch_add(resp.usage.output_tokens.unwrap_or(0), Ordering::Relaxed);
        Ok(resp.text().trim().to_string())
    }

    /// One-line summary of what a symbol does, plus a few natural-language queries it answers.
    pub async fn enrich_symbol(
        &self,
        prompts: &Prompts,
        name: &str,
        signature: &str,
        context: &str,
    ) -> Result<Enrichment> {
        let user = format!("Symbol: {name}\nSignature: {signature}\nContext: {context}");
        let raw = self
            .complete(&prompts.system(Task::Summary), &user, 160)
            .await?;
        Ok(parse_enrichment(&raw))
    }

    /// ONE section of a repo's style guide from its tailored evidence, as bullet rules without a
    /// heading; the caller adds it.
    pub async fn guide_section(
        &self,
        prompts: &Prompts,
        repo: &str,
        title: &str,
        guidance: &str,
        evidence: &str,
    ) -> Result<String> {
        let user = format!(
            "Repository: {repo}\nSection: {title}\nWhat to cover: {guidance}\n\n# Evidence\n\n{evidence}"
        );
        self.complete(&prompts.system(Task::Guide), &user, 900)
            .await
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
        let raw = self
            .complete(&prompts.system(Task::Flow), &user, 320)
            .await?;
        Ok(parse_flow(&raw))
    }
}

/// Parse the `NARRATION:` / `QUERIES:` reply format, tolerantly (narration may span lines).
fn parse_flow(raw: &str) -> (String, Vec<String>) {
    let (narr_part, q_part) = raw.split_once("QUERIES:").unwrap_or((raw, ""));
    let narration = narr_part
        .trim()
        .trim_start_matches("NARRATION:")
        .trim()
        .to_string();
    let queries = q_part
        .split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (
        if narration.is_empty() {
            raw.trim().to_string()
        } else {
            narration
        },
        queries,
    )
}

/// Parse the `SUMMARY:` / `QUERIES:` reply format, tolerantly.
fn parse_enrichment(raw: &str) -> Enrichment {
    let mut summary = String::new();
    let mut queries = Vec::new();
    for line in raw.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("SUMMARY:") {
            summary = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("QUERIES:") {
            queries = rest
                .split('|')
                .map(|q| q.trim().to_string())
                .filter(|q| !q.is_empty())
                .collect();
        }
    }
    if summary.is_empty() {
        // Model didn't follow format — fall back to the first non-empty line.
        summary = raw
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
            .to_string();
    }
    Enrichment { summary, queries }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enrichment_format() {
        let raw = "SUMMARY: Opens a pooled Postgres connection.\nQUERIES: how to connect to postgres | get a db connection | open database pool";
        let e = parse_enrichment(raw);
        assert_eq!(e.summary, "Opens a pooled Postgres connection.");
        assert_eq!(e.queries.len(), 3);
        assert_eq!(e.queries[0], "how to connect to postgres");
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
                "AsyncTaskRunner",
                "class AsyncTaskRunner(BaseTaskRunner)",
                "acme/database/executors.py",
            )
            .await
            .expect("enrich");
        assert!(!e.summary.is_empty());
        eprintln!("summary: {}", e.summary);
        eprintln!("queries: {:?}", e.queries);
    }
}
