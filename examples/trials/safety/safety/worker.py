import uuid

import boto3
from northvile_common import record_audit_event, utc_now
from northvile_common.events import unwrap
from sqlalchemy import text

from safety.config import settings
from safety.db import AdverseEvent, SessionLocal

sqs = boto3.client("sqs")

GRADE_THRESHOLD = 3


def candidate_from_flag(event_id: str, study_id: str, payload: dict) -> AdverseEvent | None:
    """A grade 3+ lab abnormality opens a candidate adverse event for the investigator to confirm."""
    if payload["grade"] < GRADE_THRESHOLD:
        return None
    with SessionLocal() as session:
        site_id = session.execute(text("SELECT site_id FROM subjects WHERE id = :s"), {"s": payload["subject_id"]}).scalar_one()
        ae = AdverseEvent(
            id=str(uuid.uuid5(uuid.NAMESPACE_URL, event_id)),
            study_id=study_id,
            subject_id=payload["subject_id"],
            site_id=site_id,
            term=f"{payload['analyte']} abnormal",
            onset_date=utc_now().date(),
            severity_grade=payload["grade"],
            aware_at=utc_now(),
            source="lab",
        )
        session.merge(ae)
        session.commit()
    record_audit_event(study_id, "safety-service", "adverse_event.candidate", "adverse_event", ae.id)
    return ae


def poll_lab_flags() -> None:
    resp = sqs.receive_message(QueueUrl=settings.lab_flags_queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=20)
    for msg in resp.get("Messages", []):
        event = unwrap(msg["Body"])
        candidate_from_flag(event.event_id, event.study_id, event.payload)
        sqs.delete_message(QueueUrl=settings.lab_flags_queue_url, ReceiptHandle=msg["ReceiptHandle"])
