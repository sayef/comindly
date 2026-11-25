import uuid
from datetime import date

from northvile_common import ServiceClient, record_audit_event, utc_now
from sqlalchemy import text

from visit_scheduler.config import settings
from visit_scheduler.db import SessionLocal, Visit
from visit_scheduler.windows import plan_visits

registry = ServiceClient(settings.registry_url, caller="visit-scheduler")


def protocol_schedule(study_id: str, protocol_version: str) -> list[dict]:
    """The schedule of the version the subject consented to, not necessarily the current one."""
    protocol = registry.get(f"/api/studies/{study_id}/protocol/versions/{protocol_version}")
    return protocol["schedule"]


def schedule_subject(study_id: str, subject_id: str, randomised_on: date, protocol_version: str) -> list[Visit]:
    visits = [
        Visit(
            id=str(uuid.uuid4()),
            subject_id=subject_id,
            study_id=study_id,
            code=w.code,
            protocol_version=protocol_version,
            target_date=w.target,
            window_start=w.start,
            window_end=w.end,
        )
        for w in plan_visits(randomised_on, protocol_schedule(study_id, protocol_version))
    ]
    with SessionLocal() as session:
        session.add_all(visits)
        session.commit()
    record_audit_event(study_id, "visit-scheduler", "visits.planned", "subject", subject_id, after={"visits": len(visits)})
    return visits


def reschedule_study(study_id: str, from_version: str, to_version: str) -> int:
    """Re-plan open visits of subjects who re-consented to the amended protocol."""
    with SessionLocal() as session:
        rows = session.execute(
            text(
                """
                SELECT s.id, s.randomised_on FROM subjects s
                WHERE s.study_id = :study AND s.protocol_version = :version AND s.status = 'enrolled'
                """
            ),
            {"study": study_id, "version": to_version},
        ).all()
        session.execute(
            text("DELETE FROM visits WHERE study_id = :study AND protocol_version = :old AND status = 'planned'"),
            {"study": study_id, "old": from_version},
        )
        session.commit()
    for subject_id, randomised_on in rows:
        schedule_subject(study_id, subject_id, randomised_on, to_version)
    return len(rows)


def cancel_open_visits(subject_id: str) -> None:
    with SessionLocal() as session:
        for visit in session.query(Visit).filter(Visit.subject_id == subject_id, Visit.status == "planned"):
            visit.status = "cancelled"
        session.commit()


def complete_visit(visit_id: str, actor: str) -> Visit:
    with SessionLocal() as session:
        visit = session.get(Visit, visit_id)
        visit.status = "completed"
        visit.completed_at = utc_now()
        session.commit()
    record_audit_event(visit.study_id, actor, "visit.completed", "visit", visit_id)
    return visit
