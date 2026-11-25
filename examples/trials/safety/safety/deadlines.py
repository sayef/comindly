from datetime import datetime, timedelta, timezone

from safety.assessment import Assessment

FATAL_DAYS = 7
OTHER_DAYS = 15


def susar_due(aware_at: datetime, assessment: Assessment) -> datetime:
    """Reporting deadline for a SUSAR.

    Day 0 is when the sponsor first became aware, in UTC. Fatal or life-threatening SUSARs are due
    within 7 days, all others within 15; the clock does not stop for missing follow-up information.
    """
    if aware_at.tzinfo is None:
        raise ValueError("awareness time must be timezone-aware")
    days = FATAL_DAYS if assessment.fatal_or_life_threatening else OTHER_DAYS
    return aware_at.astimezone(timezone.utc) + timedelta(days=days)


def days_left(due_at: datetime, now: datetime) -> int:
    return (due_at - now).days
