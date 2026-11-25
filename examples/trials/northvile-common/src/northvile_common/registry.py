import os

from northvile_common.http import ServiceClient

REGISTRY_URL = os.environ.get("TRIAL_REGISTRY_URL", "http://trial-registry")


class RegistryClient:
    """Read-only access to studies and protocols in trial-registry."""

    def __init__(self, caller: str):
        self.api = ServiceClient(REGISTRY_URL, caller=caller)

    def get_study(self, study_id: str) -> dict:
        return self.api.get(f"/api/studies/{study_id}")

    def current_protocol(self, study_id: str) -> dict:
        """The approved protocol version in force, with criteria and visit schedule."""
        return self.api.get(f"/api/studies/{study_id}/protocol/current")

    def get_site(self, site_id: str) -> dict:
        return self.api.get(f"/api/sites/{site_id}")
