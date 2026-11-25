from fastapi import APIRouter, Header, HTTPException

from visit_scheduler.db import SessionLocal, Visit
from visit_scheduler.schedule import complete_visit
from visit_scheduler.windows import VisitWindow, out_of_window

router = APIRouter(prefix="/api/v1")


@router.get("/subjects/{subject_id}/visits")
def subject_visits(subject_id: str):
    with SessionLocal() as session:
        visits = session.query(Visit).filter(Visit.subject_id == subject_id).order_by(Visit.target_date)
        return [
            {
                "id": v.id,
                "code": v.code,
                "target_date": v.target_date,
                "window": [v.window_start, v.window_end],
                "status": v.status,
            }
            for v in visits
        ]


@router.post("/visits/{visit_id}/complete")
def mark_visit_complete(visit_id: str, x_northvile_actor: str = Header()):
    """Complete a visit; an out-of-window visit is still recorded, and flagged as a deviation."""
    visit = complete_visit(visit_id, x_northvile_actor)
    if visit is None:
        raise HTTPException(404)
    window = VisitWindow(visit.code, visit.target_date, visit.window_start, visit.window_end)
    return {"id": visit_id, "status": "completed", "days_out_of_window": out_of_window(window, visit.completed_at.date())}
