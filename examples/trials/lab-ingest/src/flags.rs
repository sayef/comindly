//! Publishes `lab.result.flagged` for out-of-range values; safety turns grade 3+ into candidate AEs.

use serde_json::json;

use crate::store::LabResult;

const LAB_FLAGS_TOPIC: &str = "arn:aws:sns:eu-central-1:123456789012:lab-flags";

pub async fn publish_flag(sns: &aws_sdk_sns::Client, r: &LabResult, grade: u8) -> anyhow::Result<()> {
    let event = json!({
        "event_id": format!("flag-{}", r.id),
        "type": "lab.result.flagged",
        "study_id": r.study_id,
        "payload": {
            "subject_id": r.subject_id,
            "analyte": r.analyte,
            "value": r.value,
            "unit": r.unit,
            "grade": grade,
            "source_key": r.source_key,
        },
        "occurred_at": chrono::Utc::now().to_rfc3339(),
    });
    sns.publish()
        .topic_arn(LAB_FLAGS_TOPIC)
        .message(event.to_string())
        .send()
        .await?;
    Ok(())
}
