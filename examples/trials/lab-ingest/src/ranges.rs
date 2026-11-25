//! Reference ranges from the protocol and grading of out-of-range values.

use std::collections::HashMap;

use serde::Deserialize;

const REGISTRY: &str = "http://trial-registry/api/studies";

#[derive(Debug, Clone, Deserialize)]
pub struct LabRange {
    pub unit: String,
    pub low: f64,
    pub high: f64,
}

/// The study's lab ranges keyed by analyte code.
pub async fn fetch_ranges(client: &reqwest::Client, study_id: &str) -> anyhow::Result<HashMap<String, LabRange>> {
    let resp = client.get(format!("{REGISTRY}/{study_id}/lab-ranges")).send().await?;
    Ok(resp.error_for_status()?.json().await?)
}

/// A CTCAE-style grade from how far outside the range a value is; 0 means within range.
pub fn grade(value: f64, range: &LabRange) -> u8 {
    let ratio = if value > range.high {
        value / range.high
    } else if value < range.low {
        range.low / value.max(f64::EPSILON)
    } else {
        return 0;
    };
    match ratio {
        r if r >= 10.0 => 4,
        r if r >= 5.0 => 3,
        r if r >= 2.5 => 2,
        _ => 1,
    }
}
