from fastapi import APIRouter


class ScopedRouter(APIRouter):
    def __init__(self, *, prefix: str, scope: str):
        super().__init__(prefix=prefix)
        self.scope = scope
