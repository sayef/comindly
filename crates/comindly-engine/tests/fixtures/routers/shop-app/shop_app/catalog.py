import httpx


class CatalogClient:
    def __init__(self):
        self.http = httpx.Client(base_url="http://catalog-svc")

    def items(self):
        return self.http.get("/agent/items")
