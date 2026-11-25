import uuid

from fastapi import APIRouter, Header, HTTPException
from northvile_common import RegistryClient, record_audit_event, utc_now

from enrolment.clients import ConsentClient, RandomisationClient
from enrolment.db import EnrolmentHold, Screening, SessionLocal, Subject
from enrolment.eligibility import check_eligibility
from enrolment.events import patient_enrolled, patient_withdrawn

screenings = APIRouter(prefix="/screenings")
subjects = APIRouter(prefix="/subjects")
studies = APIRouter(prefix="/studies")

registry = RegistryClient(caller="enrolment")
consent = ConsentClient()
randomisation = RandomisationClient()


@screenings.post("")
def create_screening(body: dict, x_northvile_actor: str = Header()):
    """Open a screening at an activated site under the current protocol version."""
    site = registry.get_site(body["site_id"])
    if site["status"] != "ACTIVE":
        raise HTTPException(409, "site is not activated for this study")
    protocol = registry.current_protocol(body["study_id"])
    screening = Screening(
        id=str(uuid.uuid4()),
        study_id=body["study_id"],
        site_id=body["site_id"],
        screening_number=body["screening_number"],
        protocol_version=protocol["version"],
        created_at=utc_now(),
    )
    with SessionLocal() as session:
        session.add(screening)
        session.commit()
    record_audit_event(screening.study_id, x_northvile_actor, "screening.opened", "screening", screening.id)
    return {"id": screening.id, "protocol_version": screening.protocol_version}


@screenings.post("/{screening_id}/eligibility")
def assess_eligibility(screening_id: str, answers: dict, x_northvile_actor: str = Header()):
    with SessionLocal() as session:
        screening = session.get(Screening, screening_id)
        if screening is None:
            raise HTTPException(404)
        protocol = registry.current_protocol(screening.study_id)
        if protocol["version"] != screening.protocol_version:
            raise HTTPException(409, "protocol was amended during screening; re-screen under the new version")
        result = check_eligibility(protocol, answers)
        screening.status = "eligible" if result.eligible else "screen_failed"
        screening.failure_reasons = ", ".join(result.failed) or None
        session.commit()
    record_audit_event(screening.study_id, x_northvile_actor, "screening.assessed", "screening", screening_id, after={"status": screening.status})
    return {"eligible": result.eligible, "failed": result.failed}


@screenings.post("/{screening_id}/enrol")
def enrol(screening_id: str, body: dict, x_northvile_actor: str = Header()):
    """Enrol an eligible, consented subject: randomise, store, and announce the enrolment.

    Consent must be signed on the form for the screening's protocol version.
    """
    with SessionLocal() as session:
        screening = session.get(Screening, screening_id)
        if screening is None or screening.status != "eligible":
            raise HTTPException(409, "only eligible screenings can be enrolled")
        if session.get(EnrolmentHold, screening.study_id) is not None:
            raise HTTPException(423, "enrolment is on hold for this study")
        signed = consent.status(screening.screening_number)
        if signed.get("protocolVersion") != screening.protocol_version:
            raise HTTPException(409, "consent was signed on a different protocol version")
        subject_id = body["subject_id"]
        allocation = randomisation.randomise(screening.study_id, subject_id, body["factors"], x_northvile_actor)
        subject = Subject(
            id=subject_id,
            screening_id=screening.id,
            study_id=screening.study_id,
            site_id=screening.site_id,
            protocol_version=screening.protocol_version,
            consent_version=signed["formVersion"],
            kit_code=allocation["kit_code"],
            randomised_on=utc_now().date(),
        )
        session.add(subject)
        screening.status = "enrolled"
        session.commit()
    record_audit_event(subject.study_id, x_northvile_actor, "subject.enrolled", "subject", subject.id)
    patient_enrolled(subject.study_id, subject.id, subject.site_id, subject.randomised_on.isoformat(), subject.protocol_version)
    return {"subject_id": subject.id, "kit_code": subject.kit_code}


@subjects.get("/{subject_id}")
def get_subject(subject_id: str):
    with SessionLocal() as session:
        subject = session.get(Subject, subject_id)
        if subject is None:
            raise HTTPException(404)
        return {
            "id": subject.id,
            "study_id": subject.study_id,
            "site_id": subject.site_id,
            "status": subject.status,
            "kit_code": subject.kit_code,
            "randomised_on": subject.randomised_on,
            "protocol_version": subject.protocol_version,
        }


@subjects.post("/{subject_id}/withdraw")
def withdraw_subject(subject_id: str, body: dict, x_northvile_actor: str = Header()):
    with SessionLocal() as session:
        subject = session.get(Subject, subject_id)
        subject.status = "withdrawn"
        subject.withdrawn = True
        session.commit()
    record_audit_event(subject.study_id, x_northvile_actor, "subject.withdrawn", "subject", subject_id, reason=body["reason"])
    patient_withdrawn(subject.study_id, subject_id, body["reason"])
    return {"id": subject_id, "status": "withdrawn"}


@studies.get("/{study_id}/subjects")
def list_study_subjects(study_id: str, protocol_version: str | None = None):
    """Enrolled subjects of a study, optionally only those on one protocol version (for re-consent)."""
    with SessionLocal() as session:
        rows = session.query(Subject).filter(Subject.study_id == study_id)
        if protocol_version:
            rows = rows.filter(Subject.protocol_version == protocol_version)
        return [{"id": s.id, "site_id": s.site_id, "consent_version": s.consent_version} for s in rows]
