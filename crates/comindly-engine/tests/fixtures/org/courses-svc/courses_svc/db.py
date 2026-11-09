from sqlalchemy import Column, String
from sqlalchemy.orm import declarative_base, sessionmaker

Base = declarative_base()
SessionLocal = sessionmaker()


class Course(Base):
    __tablename__ = "courses"
    id = Column(String, primary_key=True)
    name = Column(String)
