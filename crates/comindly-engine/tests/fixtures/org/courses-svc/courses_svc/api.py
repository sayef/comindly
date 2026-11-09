from fastapi import APIRouter

from courses_svc.db import SessionLocal, Course
from courses_svc.events import publish_course_updated

router = APIRouter(prefix="/courses")


@router.get("/{course_id}")
def get_course(course_id: str):
    with SessionLocal() as session:
        return session.get(Course, course_id)


@router.post("/{course_id}")
def update_course(course_id: str, body: dict):
    with SessionLocal() as session:
        session.merge(Course(id=course_id, **body))
        session.commit()
    publish_course_updated(course_id)
