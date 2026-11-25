from northvile_common.events import Event, publish_event

AUDIT_TOPIC = "arn:aws:sns:eu-central-1:123456789012:audit-events"


def record_audit_event(
    study_id: str,
    actor: str,
    action: str,
    entity_type: str,
    entity_id: str,
    reason: str | None = None,
    before: dict | None = None,
    after: dict | None = None,
) -> str:
    """Record who changed what and why, for the 21 CFR Part 11 audit trail.

    Values must not carry the treatment arm; audit-trail is readable by site staff.
    """
    if after and "arm" in after:
        raise ValueError("audit events must not carry the treatment arm")
    payload = {
        "actor": actor,
        "action": action,
        "entity_type": entity_type,
        "entity_id": entity_id,
        "reason": reason,
        "before": before,
        "after": after,
    }
    return publish_event(AUDIT_TOPIC, Event("audit.recorded", study_id, payload))
