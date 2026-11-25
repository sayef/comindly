import re
from dataclasses import dataclass

_STUDY = re.compile(r"^NVC-\d{3}$")
_SUBJECT = re.compile(r"^(NVC-\d{3})-(\d{3})-(\d{4})$")


@dataclass(frozen=True)
class StudyId:
    value: str

    def __post_init__(self):
        if not _STUDY.match(self.value):
            raise ValueError(f"not a study id: {self.value}")


@dataclass(frozen=True)
class SubjectId:
    """Subject number `NVC-201-014-0007`: study, site number, sequence within the site."""

    study: str
    site: str
    sequence: int

    @classmethod
    def parse(cls, value: str) -> "SubjectId":
        m = _SUBJECT.match(value)
        if not m:
            raise ValueError(f"not a subject id: {value}")
        return cls(m.group(1), m.group(2), int(m.group(3)))

    def __str__(self) -> str:
        return f"{self.study}-{self.site}-{self.sequence:04d}"
