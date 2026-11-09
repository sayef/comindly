from fastapi import FastAPI

from courses_svc import api

app = FastAPI()
app.include_router(api.router, prefix="/v1")
