//! One settings document in a LanceDB table: each save is a new table version, recording who
//! changed it and when, and a save based on an older version is refused.

use std::sync::Arc;

use anyhow::{bail, Result};
use futures::TryStreamExt;
use lancedb::arrow::arrow_array::{ArrayRef, RecordBatch, StringArray};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::query::ExecutableQuery;
use serde::{de::DeserializeOwned, Serialize};

/// The document with the table version it came from and who saved it.
#[derive(Debug, Clone, Default)]
pub struct Stored<T> {
    pub doc: T,
    pub version: Option<u64>,
    pub changed_by: Option<String>,
    pub changed_at: Option<String>,
}

/// One saved version, for the history.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Version {
    pub version: u64,
    pub changed_by: String,
    pub changed_at: String,
    pub doc: serde_json::Value,
}

/// The document in table `table` at `uri`; the default when nothing was saved.
pub async fn load<T: DeserializeOwned + Default>(uri: &str, table: &str) -> Result<Stored<T>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table(table).execute().await else {
        return Ok(Stored {
            doc: T::default(),
            version: None,
            changed_by: None,
            changed_at: None,
        });
    };
    let version = tbl.version().await.ok();
    let (doc, changed_by, changed_at) = read(&tbl).await?;
    Ok(Stored {
        doc: match doc {
            Some(d) => serde_json::from_str(&d)?,
            None => T::default(),
        },
        version,
        changed_by,
        changed_at,
    })
}

async fn read(tbl: &lancedb::Table) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let mut st = tbl.query().execute().await?;
    let mut out = (None, None, None);
    while let Some(b) = st.try_next().await? {
        let col = |n: &str| crate::index::str_col(&b, n);
        if b.num_rows() > 0 {
            out = (
                Some(col("settings")?.value(0).to_string()),
                Some(col("changed_by")?.value(0).to_string()),
                Some(col("changed_at")?.value(0).to_string()),
            );
        }
    }
    Ok(out)
}

/// Save `doc` as `by` if nobody saved since `expected` (the version it was loaded at).
pub async fn save<T: Serialize>(
    uri: &str,
    table: &str,
    doc: &T,
    expected: Option<u64>,
    by: &str,
) -> Result<u64> {
    let db = crate::storage::connect(uri).execute().await?;
    let now = match db.open_table(table).execute().await {
        Ok(t) => Some(t.version().await?),
        Err(_) => None,
    };
    if now != expected {
        bail!("the settings were changed meanwhile; reload and try again");
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
            col(serde_json::to_string(doc)?),
            col(by.to_string()),
            col(chrono::Utc::now().to_rfc3339()),
        ],
    )?;
    crate::index::write_versioned(&db, table, batch).await
}

/// Every saved version, newest first.
pub async fn history(uri: &str, table: &str) -> Result<Vec<Version>> {
    let db = crate::storage::connect(uri).execute().await?;
    let Ok(tbl) = db.open_table(table).execute().await else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for v in tbl.list_versions().await? {
        tbl.checkout(v.version).await?;
        if let (Some(doc), Some(by), Some(at)) = read(&tbl).await? {
            out.push(Version {
                version: v.version,
                changed_by: by,
                changed_at: at,
                doc: serde_json::from_str(&doc).unwrap_or_default(),
            });
        }
    }
    tbl.checkout_latest().await?;
    out.reverse();
    Ok(out)
}
