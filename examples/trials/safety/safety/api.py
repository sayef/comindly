import uuid
from datetime import date

from fastapi import APIRouter, Header, HTTPException
from northvile_common import record_audit_event, utc_now
from sqlalchemy import text

from safety.assessment import assess
from safety.clients import SubjectClient
from safety.db import AdverseEvent, SessionLocal
from safety.reports import expected_terms, open_susar

router = APIRouter(prefix="/api/v1/adverse-events")
subjects = SubjectClient()


@router.post("")
def report_adverse_event(body: dict, x_northvile_actor: str = Header()):
    """Report an adverse event from a site; serious ones start the regulatory clock at once."""
    subject = subjects.get(body["subject_id"])
    if subject["status"] == "screening":
        raise HTTPException(409, "adverse events before enrolment are recorded as medical history")
    assessment = assess(body, expected_terms(subject["study_id"]))
    ae = AdverseEvent(
        id=str(uuid.uuid4()),
        study_id=subject["study_id"],
        subject_id=subject["id"],
        site_id=subject["site_id"],
        term=body["term"],
        onset_date=date.fromisoformat(body["onset_date"]),
        severity_grade=body.get("grade"),
        serious=assessment.serious,
        seriousness_criteria=",".join(assessment.criteria),
        related=assessment.related,
        expected=assessment.expected,
        aware_at=utc_now(),
        narrative=body.get("narrative"),
    )
    with SessionLocal() as session:
        session.add(ae)
        session.commit()
    record_audit_event(ae.study_id, x_northvile_actor, "adverse_event.reported", "adverse_event", ae.id)
    if assessment.susar:
        open_susar(ae, assessment, x_northvile_actor)
    return {"id": ae.id, "serious": ae.serious, "susar": assessment.susar}


@router.get("/{ae_id}")
def get_adverse_event(ae_id: str):
    with SessionLocal() as session:
        ae = session.get(AdverseEvent, ae_id)
        if ae is None:
            raise HTTPException(404)
        return {"id": ae.id, "subject_id": ae.subject_id, "term": ae.term, "serious": ae.serious, "onset_date": ae.onset_date}


@router.post("/{ae_id}/follow-ups")
def add_follow_up(ae_id: str, body: dict, x_northvile_actor: str = Header()):
    """Follow-up information never resets the reporting clock."""
    with SessionLocal() as session:
        ae = session.get(AdverseEvent, ae_id)
        ae.narrative = f"{ae.narrative or ''}\n[{utc_now().isoformat()}] {body['text']}"
        session.commit()
    record_audit_event(ae.study_id, x_northvile_actor, "adverse_event.followed_up", "adverse_event", ae_id)
    return {"id": ae_id}


def last_visit_before(subject_id: str, onset: date):
    """The last completed visit before onset, for the narrative."""
    with SessionLocal() as session:
        return session.execute(
            text(
                """
                SELECT code, completed_at FROM visits
                WHERE subject_id = :s AND status = 'completed' AND completed_at < :onset
                ORDER BY completed_at DESC LIMIT 1
                """
            ),
            {"s": subject_id, "onset": onset},
        ).first()
