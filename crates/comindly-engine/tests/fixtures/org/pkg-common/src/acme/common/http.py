import httpx


class Client:
    def __init__(self, base_url):
        self._http = httpx.Client(base_url=base_url)

    def get(self, path):
        return self._http.get(path)

    def post(self, path, body):
        return self._http.post(path, json=body)


def from_env() -> "Client":
    return Client("http://env")
