from northvile_common import Event, publish_event

from enrolment.config import TRIAL_EVENTS_TOPIC


def patient_enrolled(study_id: str, subject_id: str, site_id: str, randomised_on: str, protocol_version: str) -> str:
    """Announce an enrolment; visit-scheduler builds the visit plan from it."""
    payload = {
        "subject_id": subject_id,
        "site_id": site_id,
        "randomised_on": randomised_on,
        "protocol_version": protocol_version,
    }
    return publish_event(TRIAL_EVENTS_TOPIC, Event("patient.enrolled", study_id, payload))


def patient_withdrawn(study_id: str, subject_id: str, reason: str) -> str:
    return publish_event(TRIAL_EVENTS_TOPIC, Event("patient.withdrawn", study_id, {"subject_id": subject_id, "reason": reason}))
