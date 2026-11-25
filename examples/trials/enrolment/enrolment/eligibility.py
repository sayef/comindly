from dataclasses import dataclass, field
from datetime import date

OPERATORS = {
    ">=": lambda a, b: a >= b,
    "<=": lambda a, b: a <= b,
    "==": lambda a, b: a == b,
    "in": lambda a, b: a in b,
}


@dataclass
class EligibilityResult:
    eligible: bool
    protocol_version: str
    failed: list[str] = field(default_factory=list)


def age_on(birth_date: date, on: date) -> int:
    return on.year - birth_date.year - ((on.month, on.day) < (birth_date.month, birth_date.day))


def criterion_met(criterion: dict, answers: dict) -> bool | None:
    """True or False for a criterion; None when the site has not answered it yet."""
    value = answers.get(criterion["field"])
    if value is None:
        return None
    return OPERATORS[criterion["op"]](value, criterion["value"])


def check_eligibility(protocol: dict, answers: dict) -> EligibilityResult:
    """Evaluate the screening answers against the protocol version currently in force.

    Every inclusion criterion must hold and no exclusion criterion may; an unanswered
    criterion makes the subject ineligible rather than silently passing.
    """
    failed = []
    for c in protocol["inclusionCriteria"]:
        if criterion_met(c, answers) is not True:
            failed.append(f"inclusion {c['code']}")
    for c in protocol["exclusionCriteria"]:
        if criterion_met(c, answers) is not False:
            failed.append(f"exclusion {c['code']}")
    return EligibilityResult(not failed, protocol["version"], failed)
