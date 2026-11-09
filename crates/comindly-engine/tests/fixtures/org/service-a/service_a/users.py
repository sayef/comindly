from acme.common import get_logger, Client
from acme.common.base import BaseService
from acme.common.http import Client as HttpClient
import acme.common.events as events
from acme.common.http import from_env

log = get_logger(__name__)


class UserService(BaseService):
    def __init__(self, api: Client):
        self.api = api

    def fetch(self, user_id):
        return self.api.get(f"/users/{user_id}")

    def create(self, body):
        client = HttpClient("http://users")
        client.post("/users", body)
        events.publish("users-queue", body)
        return normalize(body)


def normalize(body):
    return body


def ping():
    client = from_env()
    return client.get("/ping")
