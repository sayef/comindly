from pypika import Query, Table


class QueryBase:
    def __init__(self):
        self.t_course = Table("courses", schema="core")


class CourseQueries(QueryBase):
    def list_courses(self):
        return Query.from_(self.t_course).select("*")

    def rename(self, course_id, name):
        return Query.update(self.t_course).set("name", name).where(self.t_course.id == course_id)
