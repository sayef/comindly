//! Team settings, stored with the index (`<index>/_team`, a versioned LanceDB table) so everyone
//! using it runs the same LLM, embedding model and LLM steps. No secrets: each member's API key
//! comes from their own credentials file or environment.

use anyhow::Result;
use serde::{Deserialize, Serialize};

const TABLE: &str = "settings";

/// What everyone using an index shares; `None` leaves comindly's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TeamSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enrich: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flows: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide: Option<bool>,
    /// Search embeddings when indexing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embed: Option<bool>,
    /// Most symbols summarised by the LLM, most central first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_enrich: Option<usize>,
    /// Most flows given a written walkthrough, furthest-reaching first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_flows: Option<usize>,
    /// LLM requests in flight at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_concurrency: Option<usize>,
    /// The LLM's price in USD per million input tokens, to show what indexing costs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_price_in: Option<f64>,
    /// The LLM's price in USD per million output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_price_out: Option<f64>,
    /// What MCP tools return as text: `md` (default) or `json`; the JSON is always attached too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// SCIP use when indexing: `auto` (default), `require` or `off`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scip: Option<String>,
    /// Re-index when a listed repository's default branch moves on its remote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_index: Option<bool>,
    /// Clone URLs of the team's repositories, so everyone can get the same set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<String>,
}

/// The stored settings with the table version they came from and who saved them.
#[derive(Debug, Clone, Default)]
pub struct Stored {
    pub settings: TeamSettings,
    pub version: Option<u64>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
}

fn uri(root: &str) -> String {
    format!("{}/_team", root.trim_end_matches('/'))
}

/// The team settings of the index at `root` (empty when none were saved).
pub async fn load(root: &str) -> Result<Stored> {
    let s = crate::docstore::load::<TeamSettings>(&uri(root), TABLE).await?;
    Ok(Stored {
        settings: s.doc,
        version: s.version,
        changed_by: s.changed_by,
        changed_at: s.changed_at,
    })
}

/// Save `settings` as `by` if nobody saved since `expected` (the version they were loaded at).
pub async fn save_as(
    root: &str,
    settings: &TeamSettings,
    expected: Option<u64>,
    by: &str,
) -> Result<u64> {
    crate::docstore::save(&uri(root), TABLE, settings, expected, by)
        .await
        .map_err(|e| match format!("{e}").contains("changed meanwhile") {
            true => anyhow::anyhow!(
                "the team settings were changed by someone else; reload and try again"
            ),
            false => e,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_load_and_refuse_a_stale_write() {
        let dir = std::env::temp_dir().join(format!("comindly-team-{}", std::process::id()));
        let root = dir.to_str().unwrap();
        let first = TeamSettings {
            llm_model: Some("m1".into()),
            ..Default::default()
        };
        let v1 = save_as(root, &first, None, "t").await.unwrap();
        assert_eq!(load(root).await.unwrap().settings, first);
        let second = TeamSettings {
            llm_model: Some("m2".into()),
            ..Default::default()
        };
        save_as(root, &second, Some(v1), "t").await.unwrap();
        assert!(
            save_as(root, &first, Some(v1), "t").await.is_err(),
            "stale version"
        );
        assert_eq!(
            load(root).await.unwrap().settings.llm_model.as_deref(),
            Some("m2")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
