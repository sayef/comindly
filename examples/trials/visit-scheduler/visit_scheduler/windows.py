from dataclasses import dataclass
from datetime import date, timedelta


@dataclass(frozen=True)
class VisitWindow:
    code: str
    target: date
    start: date
    end: date

    def contains(self, day: date) -> bool:
        return self.start <= day <= self.end


def plan_visits(randomised_on: date, schedule: list[dict]) -> list[VisitWindow]:
    """Visit windows from the protocol schedule; day 1 is the randomisation day."""
    windows = []
    for visit in sorted(schedule, key=lambda v: v["day"]):
        target = randomised_on + timedelta(days=visit["day"] - 1)
        windows.append(
            VisitWindow(
                code=visit["code"],
                target=target,
                start=target - timedelta(days=visit["windowBefore"]),
                end=target + timedelta(days=visit["windowAfter"]),
            )
        )
    return windows


def out_of_window(window: VisitWindow, visit_day: date) -> int:
    """Days outside the window (negative when early); 0 when inside. Out-of-window visits are deviations."""
    if visit_day < window.start:
        return (visit_day - window.start).days
    if visit_day > window.end:
        return (visit_day - window.end).days
    return 0
