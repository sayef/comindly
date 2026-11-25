from fastapi import FastAPI

from visit_scheduler.api import router

app = FastAPI(title="visit-scheduler")
app.include_router(router)
