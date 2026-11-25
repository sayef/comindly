import httpx

DEFAULT_TIMEOUT = httpx.Timeout(5.0, connect=2.0)


class ServiceClient:
    """HTTP client for another Northvile service: retries, timeouts and the caller's service name."""

    def __init__(self, base_url: str, caller: str = "unknown", timeout: httpx.Timeout = DEFAULT_TIMEOUT):
        transport = httpx.HTTPTransport(retries=2)
        self._http = httpx.Client(
            base_url=base_url,
            timeout=timeout,
            transport=transport,
            headers={"x-northvile-caller": caller},
        )

    def get(self, path: str, **params) -> dict:
        resp = self._http.get(path, params=params or None)
        resp.raise_for_status()
        return resp.json()

    def post(self, path: str, body: dict) -> dict:
        resp = self._http.post(path, json=body)
        resp.raise_for_status()
        return resp.json() if resp.content else {}

    def put(self, path: str, body: dict) -> dict:
        resp = self._http.put(path, json=body)
        resp.raise_for_status()
        return resp.json()
