import json
from datetime import date, timedelta

import boto3
from northvile_common import utc_now
from northvile_common.events import unwrap

from visit_scheduler.config import settings
from visit_scheduler.db import ProcessedEvent, SessionLocal, Visit
from visit_scheduler.schedule import cancel_open_visits, reschedule_study, schedule_subject

sqs = boto3.client("sqs")


def already_processed(event_id: str) -> bool:
    with SessionLocal() as session:
        if session.get(ProcessedEvent, event_id) is not None:
            return True
        session.add(ProcessedEvent(event_id=event_id, processed_at=utc_now()))
        session.commit()
        return False


def handle_trial_event(body: str) -> None:
    event = unwrap(body)
    if already_processed(event.event_id):
        return
    p = event.payload
    if event.type == "patient.enrolled":
        schedule_subject(event.study_id, p["subject_id"], date.fromisoformat(p["randomised_on"]), p["protocol_version"])
    elif event.type == "patient.withdrawn":
        cancel_open_visits(p["subject_id"])


def handle_protocol_change(body: str) -> None:
    amendment = json.loads(json.loads(body)["Message"])
    if amendment["affectsSchedule"]:
        reschedule_study(amendment["studyId"], amendment["fromVersion"], amendment["toVersion"])


def poll_trial_events() -> None:
    resp = sqs.receive_message(QueueUrl=settings.scheduling_queue_url, MaxNumberOfMessages=10, WaitTimeSeconds=20)
    for msg in resp.get("Messages", []):
        handle_trial_event(msg["Body"])
        sqs.delete_message(QueueUrl=settings.scheduling_queue_url, ReceiptHandle=msg["ReceiptHandle"])


def poll_protocol_changes() -> None:
    resp = sqs.receive_message(QueueUrl=settings.protocol_changes_queue_url, WaitTimeSeconds=20)
    for msg in resp.get("Messages", []):
        handle_protocol_change(msg["Body"])
        sqs.delete_message(QueueUrl=settings.protocol_changes_queue_url, ReceiptHandle=msg["ReceiptHandle"])


def send_reminders(today: date) -> int:
    """Queue reminders for visits whose window opens soon; the notification service sends them."""
    horizon = today + timedelta(days=settings.reminder_days_ahead)
    sent = 0
    with SessionLocal() as session:
        due = session.query(Visit).filter(Visit.status == "planned", Visit.window_start == horizon)
        for visit in due:
            sqs.send_message(
                QueueUrl=settings.reminders_queue_url,
                MessageBody=json.dumps({"visit_id": visit.id, "subject_id": visit.subject_id, "window_start": horizon.isoformat()}),
            )
            sent += 1
    return sent
