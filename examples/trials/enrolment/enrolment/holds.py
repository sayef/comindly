import boto3
from northvile_common.events import unwrap
from northvile_common import utc_now

from enrolment.config import SAFETY_HOLDS_QUEUE
from enrolment.db import EnrolmentHold, SessionLocal

sqs = boto3.client("sqs")


def apply_safety_event(event_type: str, study_id: str, payload: dict) -> None:
    """An urgent safety measure stops new enrolments at once; lifting it is a manual decision."""
    if event_type != "urgent_safety_measure.raised":
        return
    with SessionLocal() as session:
        session.merge(EnrolmentHold(study_id=study_id, reason=payload["summary"], since=utc_now()))
        session.commit()


def poll() -> None:
    resp = sqs.receive_message(QueueUrl=SAFETY_HOLDS_QUEUE, MaxNumberOfMessages=10, WaitTimeSeconds=20)
    for msg in resp.get("Messages", []):
        event = unwrap(msg["Body"])
        apply_safety_event(event.type, event.study_id, event.payload)
        sqs.delete_message(QueueUrl=SAFETY_HOLDS_QUEUE, ReceiptHandle=msg["ReceiptHandle"])
