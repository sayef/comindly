from fastapi import FastAPI

from safety.api import router

app = FastAPI(title="safety")
app.include_router(router)
