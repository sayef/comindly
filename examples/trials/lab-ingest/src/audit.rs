//! Records ingested files in audit-trail.

use serde_json::json;

const AUDIT_EVENTS: &str = "http://audit-trail/api/audit/events";

pub async fn file_ingested(client: &reqwest::Client, study_id: &str, key: &str, rows: usize) -> anyhow::Result<()> {
    client
        .post(AUDIT_EVENTS)
        .json(&json!({
            "id": format!("lab-file-{key}"),
            "study_id": study_id,
            "actor": "lab-ingest",
            "action": "lab_file.ingested",
            "entity_type": "lab_file",
            "entity_id": key,
            "after": { "rows": rows },
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
