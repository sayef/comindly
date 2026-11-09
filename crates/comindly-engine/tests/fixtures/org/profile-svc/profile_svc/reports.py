from sqlalchemy import text


def count_courses(conn):
    return conn.execute(text("SELECT count(*) FROM courses")).scalar()
