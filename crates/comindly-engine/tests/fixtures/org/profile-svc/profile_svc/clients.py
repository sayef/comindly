import httpx

from profile_svc.config import settings


class CoursesClient:
    def __init__(self):
        self.http = httpx.Client(base_url=settings.courses_api_url)

    def fetch(self, course_id: str):
        return self.http.get(f"/v1/courses/{course_id}")

    def rename(self, course_id: str, name: str):
        return self.http.post(f"/v1/courses/{course_id}", json={"name": name})
