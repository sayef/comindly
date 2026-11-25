import boto3
from northvile_common import record_audit_event, utc_now

from safety.config import settings
from safety.db import AdverseEvent, SafetyReport, SessionLocal
from safety.deadlines import days_left

s3 = boto3.client("s3")


def due_reports() -> list[SafetyReport]:
    """Unsubmitted expedited reports, most urgent first."""
    with SessionLocal() as session:
        return session.query(SafetyReport).filter(SafetyReport.submitted_at.is_(None)).order_by(SafetyReport.due_at).all()


def submit(report_id: str, actor: str, gateway_ref: str) -> SafetyReport:
    """Mark a report as sent to the authority; the clock stops at submission, not acknowledgement."""
    with SessionLocal() as session:
        report = session.get(SafetyReport, report_id)
        ae = session.get(AdverseEvent, report.adverse_event_id)
        s3.head_object(Bucket=settings.reports_bucket, Key=report.document_key)
        report.submitted_at = utc_now()
        session.commit()
    late = days_left(report.due_at, report.submitted_at) < 0
    record_audit_event(ae.study_id, actor, "safety_report.submitted", "safety_report", report_id, reason=gateway_ref, after={"late": late})
    return report
