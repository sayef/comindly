import httpx
from northvile_common.http import DEFAULT_TIMEOUT

from enrolment.config import CONSENT_URL, RANDOMISATION_URL


class ConsentClient:
    """Signed consent per subject, from the consent service."""

    def __init__(self):
        self.http = httpx.Client(base_url=CONSENT_URL, timeout=DEFAULT_TIMEOUT)

    def status(self, subject_ref: str) -> dict:
        resp = self.http.get(f"/api/consent/subjects/{subject_ref}")
        resp.raise_for_status()
        return resp.json()


class RandomisationClient:
    """Blinded allocation. The response carries a kit code, never the arm."""

    def __init__(self):
        self.http = httpx.Client(base_url=RANDOMISATION_URL, timeout=DEFAULT_TIMEOUT)

    def randomise(self, study_id: str, subject_id: str, factors: dict, actor: str) -> dict:
        resp = self.http.post(
            "/api/randomisations",
            json={"study_id": study_id, "subject_id": subject_id, "factors": factors, "actor": actor},
        )
        resp.raise_for_status()
        body = resp.json()
        if "arm" in body:
            raise RuntimeError("randomisation returned an unblinded response")
        return body
