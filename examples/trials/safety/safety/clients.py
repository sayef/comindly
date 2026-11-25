import httpx
from northvile_common.http import DEFAULT_TIMEOUT

from safety.config import settings


class UnblindingClient:
    """Emergency unblinding for expedited reporting. Only the safety physician role may call it."""

    def __init__(self):
        self.http = httpx.Client(base_url=settings.randomisation_url, timeout=DEFAULT_TIMEOUT)

    def unblind(self, subject_id: str, actor: str, reason: str) -> str:
        resp = self.http.post(
            f"/api/randomisations/{subject_id}/unblind",
            json={"role": "SAFETY_PHYSICIAN", "actor": actor, "reason": reason},
        )
        resp.raise_for_status()
        return resp.json()["arm"]


class LabClient:
    def __init__(self):
        self.http = httpx.Client(base_url=settings.lab_ingest_url, timeout=DEFAULT_TIMEOUT)

    def flags(self, subject_id: str) -> list[dict]:
        resp = self.http.get(f"/api/lab-results/{subject_id}/flags")
        resp.raise_for_status()
        return resp.json()


class SubjectClient:
    def __init__(self):
        self.http = httpx.Client(base_url=settings.enrolment_url, timeout=DEFAULT_TIMEOUT)

    def get(self, subject_id: str) -> dict:
        resp = self.http.get(f"/api/v1/subjects/{subject_id}")
        resp.raise_for_status()
        return resp.json()
