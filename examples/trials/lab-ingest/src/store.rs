//! lab_results in Postgres. Rows are corrected by inserting a new version, never updated in place.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LabResult {
    pub id: String,
    pub study_id: String,
    pub subject_id: String,
    pub analyte: String,
    pub value: f64,
    pub unit: String,
    pub collected_at: DateTime<Utc>,
    pub flag: Option<i16>,
    pub source_key: String,
    pub version: i32,
}

pub struct ResultStore {
    pool: PgPool,
}

impl ResultStore {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Ok(Self { pool: PgPool::connect(url).await? })
    }

    /// Inserts a result; the same source file and line yield the same id, so re-ingesting is a no-op.
    pub async fn insert(&self, r: &LabResult) -> anyhow::Result<bool> {
        let done = sqlx::query(
            "INSERT INTO lab_results (id, study_id, subject_id, analyte, value, unit, collected_at, flag, source_key, version) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) ON CONFLICT (id) DO NOTHING",
        )
        .bind(&r.id)
        .bind(&r.study_id)
        .bind(&r.subject_id)
        .bind(&r.analyte)
        .bind(r.value)
        .bind(&r.unit)
        .bind(r.collected_at)
        .bind(r.flag)
        .bind(&r.source_key)
        .bind(r.version)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn for_subject(&self, subject_id: &str) -> anyhow::Result<Vec<LabResult>> {
        Ok(sqlx::query_as("SELECT * FROM lab_results WHERE subject_id = $1 ORDER BY collected_at")
            .bind(subject_id)
            .fetch_all(&self.pool)
            .await?)
    }
}
