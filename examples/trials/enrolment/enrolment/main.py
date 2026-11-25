from fastapi import FastAPI

from enrolment.api import screenings, studies, subjects

app = FastAPI(title="enrolment")
app.include_router(screenings, prefix="/api/v1")
app.include_router(subjects, prefix="/api/v1")
app.include_router(studies, prefix="/api/v1")
