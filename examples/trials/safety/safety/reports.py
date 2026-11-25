import json
import uuid

import boto3
from northvile_common import Event, RegistryClient, publish_event, record_audit_event, utc_now

from safety.assessment import Assessment
from safety.clients import UnblindingClient
from safety.config import settings
from safety.db import AdverseEvent, SafetyReport, SessionLocal
from safety.deadlines import susar_due

s3 = boto3.client("s3")
registry = RegistryClient(caller="safety")
unblinding = UnblindingClient()


def expected_terms(study_id: str) -> set[str]:
    rsi = registry.api.get(f"/api/studies/{study_id}/reference-safety-information")
    return {t.lower() for t in rsi["expectedTerms"]}


def open_susar(ae: AdverseEvent, assessment: Assessment, actor: str) -> SafetyReport:
    """Open a SUSAR report and start its clock.

    The authorities need the actual treatment, so this is the one place that unblinds; the arm goes
    into the report document only, never into the database, events or the audit trail.
    """
    arm = unblinding.unblind(ae.subject_id, actor, reason=f"SUSAR {ae.id}")
    report = SafetyReport(id=str(uuid.uuid4()), adverse_event_id=ae.id, kind="SUSAR", due_at=susar_due(ae.aware_at, assessment))
    document = {
        "report_id": report.id,
        "study_id": ae.study_id,
        "subject_id": ae.subject_id,
        "term": ae.term,
        "criteria": list(assessment.criteria),
        "treatment": arm,
        "due_at": report.due_at.isoformat(),
    }
    report.document_key = f"{ae.study_id}/susar/{report.id}.json"
    s3.put_object(Bucket=settings.reports_bucket, Key=report.document_key, Body=json.dumps(document), ServerSideEncryption="aws:kms")
    with SessionLocal() as session:
        session.add(report)
        session.commit()
    record_audit_event(ae.study_id, actor, "susar.opened", "adverse_event", ae.id, after={"due_at": report.due_at.isoformat()})
    publish_event(
        settings.safety_events_topic_arn,
        Event("susar.opened", ae.study_id, {"adverse_event_id": ae.id, "subject_id": ae.subject_id, "due_at": report.due_at.isoformat()}),
    )
    return report


def raise_urgent_safety_measure(study_id: str, summary: str, actor: str) -> str:
    """Stop enrolment study-wide; enrolment applies the hold when it sees this event."""
    record_audit_event(study_id, actor, "urgent_safety_measure.raised", "study", study_id, reason=summary)
    return publish_event(settings.safety_events_topic_arn, Event("urgent_safety_measure.raised", study_id, {"summary": summary, "raised_at": utc_now().isoformat()}))


def source_lab_file(key: str) -> bytes:
    """The original lab vendor file behind a flag, for the medical reviewer."""
    return s3.get_object(Bucket=settings.lab_drops_bucket, Key=key)["Body"].read()
