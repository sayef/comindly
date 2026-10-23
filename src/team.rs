//! Team settings, stored with the index (`<index>/_team`, a versioned LanceDB table) so everyone
//! using it runs the same LLM, embedding model and LLM steps. No secrets: each member's API key
//! comes from their own credentials file or environment.

use std::sync::{Arc, OnceLock, RwLock};

use anyhow::{bail, Result};
use futures::TryStreamExt;
use lancedb::arrow::arrow_array::{ArrayRef, RecordBatch, StringArray};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::query::ExecutableQuery;
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
    let db = crate::storage::connect(&uri(root)).execute().await?;
    let Ok(tbl) = db.open_table(TABLE).execute().await else {
        return Ok(Stored::default());
    };
    let version = tbl.version().await.ok();
    let mut st = tbl.query().execute().await?;
    let mut out = Stored {
        version,
        ..Default::default()
    };
    while let Some(b) = st.try_next().await? {
        let col = |n: &str| crate::index::str_col(&b, n);
        if b.num_rows() > 0 {
            out.settings = serde_json::from_str(col("settings")?.value(0))?;
            out.changed_by = Some(col("changed_by")?.value(0).to_string());
            out.changed_at = Some(col("changed_at")?.value(0).to_string());
        }
    }
    Ok(out)
}

/// Save `settings` if nobody saved since `expected` (the version they were loaded at).
pub async fn save(root: &str, settings: &TeamSettings, expected: Option<u64>) -> Result<u64> {
    let by = crate::ledger::author_name().unwrap_or_else(crate::ledger::actor);
    save_as(root, settings, expected, &by).await
}

/// [`save`], recorded as changed by `by`.
pub async fn save_as(
    root: &str,
    settings: &TeamSettings,
    expected: Option<u64>,
    by: &str,
) -> Result<u64> {
    let by = by.to_string();
    let db = crate::storage::connect(&uri(root)).execute().await?;
    let now = match db.open_table(TABLE).execute().await {
        Ok(t) => Some(t.version().await?),
        Err(_) => None,
    };
    if now != expected {
        bail!("the team settings were changed by someone else; reload and try again");
    }
    let schema = Arc::new(Schema::new(vec![
        Field::new("settings", DataType::Utf8, false),
        Field::new("changed_by", DataType::Utf8, false),
        Field::new("changed_at", DataType::Utf8, false),
    ]));
    let col = |v: String| Arc::new(StringArray::from(vec![v])) as ArrayRef;
    let batch = RecordBatch::try_new(
        schema,
        vec![
            col(serde_json::to_string(settings)?),
            col(by),
            col(chrono::Utc::now().to_rfc3339()),
        ],
    )?;
    crate::index::write_versioned(&db, TABLE, batch).await
}

fn active() -> &'static RwLock<Option<TeamSettings>> {
    static A: OnceLock<RwLock<Option<TeamSettings>>> = OnceLock::new();
    A.get_or_init(|| RwLock::new(None))
}

/// The team settings this process runs with, if its index has any.
pub fn current() -> Option<TeamSettings> {
    active().read().ok().and_then(|a| a.clone())
}

/// Use the team settings of the index at `root` in this process.
pub fn apply(root: &str) -> Result<()> {
    let stored = crate::rt::block_on(load(root))?;
    set_current(stored.settings);
    Ok(())
}

/// Replace the process's team settings (after a save or a switch from the UI).
pub fn set_current(settings: TeamSettings) {
    if let Ok(mut g) = active().write() {
        *g = Some(settings);
    }
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
        let v1 = save(root, &first, None).await.unwrap();
        assert_eq!(load(root).await.unwrap().settings, first);
        let second = TeamSettings {
            llm_model: Some("m2".into()),
            ..Default::default()
        };
        save(root, &second, Some(v1)).await.unwrap();
        assert!(save(root, &first, Some(v1)).await.is_err(), "stale version");
        assert_eq!(
            load(root).await.unwrap().settings.llm_model.as_deref(),
            Some("m2")
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
