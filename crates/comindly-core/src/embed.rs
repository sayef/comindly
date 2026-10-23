//! Comindly embed — local static embeddings for semantic code search.
//!
//! Uses **Model2Vec** (`model2vec-rs`) static embeddings: pure-Rust, CPU-only, no ONNX
//! runtime, tiny + fast (~ms/query) — the approach semble validated for agentic code
//! search. Vectors are L2-normalized, so cosine similarity is a plain dot product.
//!
//! This is the *optional semantic layer*. It rides on top of the deterministic graph:
//! search fuses this signal with lexical BM25 and graph centrality (see comindly's hybrid
//! search) — never the sole retrieval path.

use crate::model::Symbol;
use anyhow::{Context, Result};
use model2vec_rs::model::StaticModel;

/// A small, well-known static model. Swap for a code-specialized Model2Vec distillation
/// (e.g. a `potion-code` variant) for higher code accuracy — same API.
pub const DEFAULT_MODEL: &str = "minishlab/potion-base-8M";

/// Loads a Model2Vec model (from a Hugging Face repo id or a local path) and encodes text.
pub struct Embedder {
    model: StaticModel,
}

impl Embedder {
    /// Load a model by HF repo id or local path. Normalizes output (cosine == dot product).
    pub fn load(repo_or_path: &str) -> Result<Self> {
        let model = StaticModel::from_pretrained(repo_or_path, None, Some(true), None)
            .with_context(|| format!("load Model2Vec model `{repo_or_path}`"))?;
        Ok(Self { model })
    }

    /// Embed a batch of documents.
    pub fn embed(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.model.encode(texts)
    }

    /// Embed a single query string.
    pub fn embed_query(&self, query: &str) -> Vec<f32> {
        self.model.encode_single(query)
    }
}

/// Build the text used to represent a symbol for embedding: name, signature, the
/// human-readable descriptor path, and docstring — the tokens a natural-language query
/// about this code would plausibly match.
pub fn symbol_text(s: &Symbol) -> String {
    let mut parts = vec![s.name.clone()];
    if let Some(sig) = &s.signature {
        parts.push(sig.clone());
    }
    // descriptor path as words, e.g. "acme/database/config/DbConfig" -> readable tokens
    parts.push(s.id.descriptor.replace(['/', '_', '.'], " "));
    if let Some(doc) = &s.docstring {
        parts.push(doc.clone());
    }
    parts.join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    // Downloads a model from Hugging Face — run explicitly: `cargo test -p comindly-embed -- --ignored`.
    #[test]
    #[ignore]
    fn embeds_and_ranks_semantically() {
        let e = Embedder::load(DEFAULT_MODEL).expect("load model");
        let docs: Vec<String> = vec![
            "open_database_connection  def open_database_connection(host, port)".into(),
            "parse_json  def parse_json(text: str) -> dict".into(),
            "UserAuthentication  class UserAuthentication  login verify password".into(),
        ];
        let vecs = e.embed(&docs);
        assert_eq!(vecs.len(), 3);
        assert!(!vecs[0].is_empty());

        // A query about auth should rank the authentication doc highest.
        let q = e.embed_query("log in and check a user's password");
        let best = (0..3)
            .max_by(|&i, &j| dot(&q, &vecs[i]).partial_cmp(&dot(&q, &vecs[j])).unwrap())
            .unwrap();
        assert_eq!(best, 2, "auth doc should rank first");
    }
}

/// Code-aware ranking signals for hybrid search: identifier splitting, lexical scoring,
/// query classification, and path penalties. Ideas borrowed from semble (re-implemented,
/// not copied); comindly adds a dependency-graph centrality signal on top (applied by the caller).
pub mod rank {
    use std::collections::BTreeSet;

    /// Split an identifier into lowercased sub-tokens, camelCase + snake_case aware, keeping the
    /// compound form. `getHTTPResponse` -> {gethttpresponse, get, http, response};
    /// `my_func` -> {my_func, my, func}.
    pub fn split_identifier(ident: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![ident.to_lowercase()];
        // snake_case
        let snake: Vec<&str> = ident.split('_').filter(|s| !s.is_empty()).collect();
        let mut parts: Vec<String> = Vec::new();
        for seg in snake {
            parts.extend(split_camel(seg));
        }
        for p in parts {
            let p = p.to_lowercase();
            if p.len() >= 2 && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// Split a camelCase/PascalCase run into words: `getHTTPResponse` -> [get, HTTP, Response].
    fn split_camel(s: &str) -> Vec<String> {
        let mut words = Vec::new();
        let chars: Vec<char> = s.chars().collect();
        let mut start = 0;
        for i in 1..chars.len() {
            let (prev, cur) = (chars[i - 1], chars[i]);
            // boundary: lower->UPPER, or UPPER->UPPER followed by lower (acronym end), or letter<->digit
            let boundary = (prev.is_lowercase() && cur.is_uppercase())
                || (prev.is_uppercase()
                    && cur.is_uppercase()
                    && i + 1 < chars.len()
                    && chars[i + 1].is_lowercase())
                || (prev.is_alphabetic() != cur.is_alphabetic());
            if boundary {
                words.push(chars[start..i].iter().collect());
                start = i;
            }
        }
        if start < chars.len() {
            words.push(chars[start..].iter().collect());
        }
        words
    }

    const STOPWORDS: &[&str] = &[
        "the", "a", "an", "of", "to", "in", "and", "or", "for", "is", "how", "do", "does", "get",
        "that", "this", "with", "on", "by",
    ];

    /// Extract meaningful lowercased query keywords (>2 chars, non-stopword).
    pub fn query_keywords(query: &str) -> Vec<String> {
        query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .flat_map(split_identifier)
            .filter(|w| w.len() > 2 && !STOPWORDS.contains(&w.as_str()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// Symbol-like query? (identifier path, snake/camel case, punctuation, or a single word)
    /// Such queries weight lexical matching higher (α smaller).
    pub fn is_symbol_query(query: &str) -> bool {
        let q = query.trim();
        if q.contains("::") || q.contains('.') || q.contains('_') || q.contains('/') {
            return true;
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        if words.len() == 1 {
            let w = words[0];
            // camelCase / PascalCase / has an uppercase interior letter
            return w.chars().any(|c| c.is_uppercase()) || w.len() <= 24;
        }
        false
    }

    /// Weight on the *semantic* signal (the rest goes to lexical). Symbol queries favor lexical.
    pub fn resolve_alpha(query: &str) -> f32 {
        if is_symbol_query(query) {
            0.3
        } else {
            0.5
        }
    }

    /// Lexical overlap in `[0,1]`: fraction of query keywords matching the symbol's identifier
    /// sub-tokens (prefix match allowed for stems ≥4 chars).
    pub fn lexical_score(keywords: &[String], name: &str, descriptor_tail: &str) -> f32 {
        if keywords.is_empty() {
            return 0.0;
        }
        let mut toks: BTreeSet<String> = BTreeSet::new();
        toks.extend(split_identifier(name));
        toks.extend(split_identifier(descriptor_tail));
        let hits = keywords
            .iter()
            .filter(|k| {
                toks.contains(*k)
                    || (k.len() >= 4 && toks.iter().any(|t| t.starts_with(k.as_str())))
            })
            .count();
        hits as f32 / keywords.len() as f32
    }

    /// Path-based noise penalty multiplier (semble's structured penalties).
    pub fn path_penalty(path: &str) -> f32 {
        let p = path.to_lowercase();
        let is_test = p.contains("/test")
            || p.contains("test_")
            || p.contains("_test.")
            || p.contains(".test.")
            || p.contains(".spec.")
            || p.contains("/tests/")
            || p.contains("/spec/")
            || p.contains("__tests__");
        if is_test
            || p.contains("/examples")
            || p.contains("/legacy")
            || p.contains("/compat")
            || p.contains("/migration")
            || p.contains("/revision")
        {
            return 0.3;
        }
        if p.ends_with("/__init__.py") || p.ends_with("package-info.java") || p.ends_with("mod.rs")
        {
            return 0.5; // re-export barrels
        }
        if p.ends_with(".d.ts") {
            return 0.7;
        }
        1.0
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn splits_identifiers() {
            let t = split_identifier("AsyncTaskRunner");
            assert!(t.contains(&"async".to_string()));
            assert!(t.contains(&"task".to_string()));
            assert!(t.contains(&"runner".to_string()));
            let s = split_identifier("get_user_id");
            assert!(s.contains(&"user".to_string()) && s.contains(&"get_user_id".to_string()));
        }

        #[test]
        fn detects_symbol_queries() {
            assert!(is_symbol_query("AsyncTaskRunner"));
            assert!(is_symbol_query("acme.const.Settings"));
            assert!(is_symbol_query("get_config"));
            assert!(!is_symbol_query("how do we connect to the database"));
        }

        #[test]
        fn lexical_and_penalty() {
            let kw = query_keywords("task runner");
            assert!(lexical_score(&kw, "AsyncTaskRunner", "acme/database/runners") > 0.9);
            assert_eq!(path_penalty("app/tests/test_api.py"), 0.3);
            assert_eq!(path_penalty("acme/db.py"), 1.0);
        }
    }
}
