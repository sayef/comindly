from datetime import date, datetime, timezone


def utc_now() -> datetime:
    """The current time in UTC; services never store local times."""
    return datetime.now(timezone.utc)


def study_day(reference: date, on: date) -> int:
    """Study day of `on` relative to `reference` (day 1 is the reference day, there is no day 0)."""
    delta = (on - reference).days
    return delta + 1 if delta >= 0 else delta


def parse_utc(value: str) -> datetime:
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError(f"timestamp without a zone: {value}")
    return parsed.astimezone(timezone.utc)
