from dataclasses import dataclass

SERIOUSNESS_CRITERIA = (
    "death",
    "life_threatening",
    "hospitalisation",
    "disability",
    "congenital_anomaly",
    "important_medical_event",
)


@dataclass(frozen=True)
class Assessment:
    serious: bool
    criteria: tuple[str, ...]
    related: bool
    expected: bool

    @property
    def susar(self) -> bool:
        """Suspected unexpected serious adverse reaction: serious, related and not expected."""
        return self.serious and self.related and not self.expected

    @property
    def fatal_or_life_threatening(self) -> bool:
        return "death" in self.criteria or "life_threatening" in self.criteria


def assess(report: dict, expected_terms: set[str]) -> Assessment:
    """Seriousness from the reported criteria, expectedness against the reference safety information.

    Causality is the investigator's judgement; when it is missing the event is treated as related.
    """
    criteria = tuple(c for c in SERIOUSNESS_CRITERIA if report.get(c))
    related = report.get("causality", "related") != "not_related"
    expected = report["term"].lower() in expected_terms
    return Assessment(bool(criteria), criteria, related, expected)
