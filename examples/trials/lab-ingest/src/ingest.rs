//! Reads vendor CSV files dropped in the lab bucket, normalises units, grades and stores each result.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::store::LabResult;
use crate::units::{normalise, Quantity};
use crate::{audit, flags, ranges, App};

const LAB_DROPS: &str = "northvile-prod-lab-drops";

#[derive(Debug, Deserialize)]
struct VendorRow {
    study_id: String,
    subject_id: String,
    analyte: String,
    value: f64,
    unit: String,
    collected_at: DateTime<Utc>,
}

pub async fn run(app: Arc<App>) {
    let http = reqwest::Client::new();
    loop {
        if let Err(e) = sweep(&app, &http).await {
            eprintln!("lab sweep failed: {e:#}");
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn sweep(app: &App, http: &reqwest::Client) -> anyhow::Result<()> {
    let listing = app.s3.list_objects_v2().bucket(LAB_DROPS).prefix("incoming/").send().await?;
    for obj in listing.contents() {
        if let Some(key) = obj.key() {
            ingest_file(app, http, key).await?;
        }
    }
    Ok(())
}

/// One vendor file. Rows that cannot be converted to the study's unit stop the file, not just the row.
pub async fn ingest_file(app: &App, http: &reqwest::Client, key: &str) -> anyhow::Result<usize> {
    let body = app.s3.get_object().bucket(LAB_DROPS).key(key).send().await?.body.collect().await?;
    let mut reader = csv::Reader::from_reader(body.into_bytes().as_ref());
    let rows: Vec<VendorRow> = reader.deserialize().collect::<Result<_, _>>()?;
    let Some(study_id) = rows.first().map(|r| r.study_id.clone()) else {
        return Ok(0);
    };
    let study_ranges = ranges::fetch_ranges(http, &study_id).await?;
    let mut stored = 0;
    for (line, row) in rows.iter().enumerate() {
        let range = study_ranges
            .get(&row.analyte)
            .ok_or_else(|| anyhow::anyhow!("{key}:{line}: no range for {}", row.analyte))?;
        let q = normalise(&row.analyte, &Quantity { value: row.value, unit: row.unit.clone() }, &range.unit)
            .map_err(|e| anyhow::anyhow!("{key}:{line}: {e}"))?;
        let grade = ranges::grade(q.value, range);
        let result = LabResult {
            id: format!("{key}#{line}"),
            study_id: row.study_id.clone(),
            subject_id: row.subject_id.clone(),
            analyte: row.analyte.clone(),
            value: q.value,
            unit: q.unit,
            collected_at: row.collected_at,
            flag: (grade > 0).then_some(grade as i16),
            source_key: key.to_string(),
            version: 1,
        };
        if app.store.insert(&result).await? {
            stored += 1;
            if grade > 0 {
                flags::publish_flag(&app.sns, &result, grade).await?;
            }
        }
    }
    audit::file_ingested(http, &study_id, key, stored).await?;
    Ok(stored)
}
