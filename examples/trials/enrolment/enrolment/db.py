from sqlalchemy import Boolean, Column, Date, DateTime, ForeignKey, String
from sqlalchemy.orm import declarative_base, relationship, sessionmaker

Base = declarative_base()
SessionLocal = sessionmaker()


class Screening(Base):
    __tablename__ = "screenings"
    id = Column(String, primary_key=True)
    study_id = Column(String, nullable=False)
    site_id = Column(String, nullable=False)
    screening_number = Column(String, nullable=False, unique=True)
    protocol_version = Column(String, nullable=False)
    status = Column(String, default="screening")
    failure_reasons = Column(String)
    created_at = Column(DateTime(timezone=True), nullable=False)
    subject = relationship("Subject", back_populates="screening", uselist=False)


class Subject(Base):
    """An enrolled participant. Holds no treatment arm: the blinded kit code is all we keep."""

    __tablename__ = "subjects"
    id = Column(String, primary_key=True)
    screening_id = Column(String, ForeignKey("screenings.id"), unique=True)
    study_id = Column(String, nullable=False)
    site_id = Column(String, nullable=False)
    protocol_version = Column(String, nullable=False)
    consent_version = Column(String, nullable=False)
    kit_code = Column(String)
    randomised_on = Column(Date)
    status = Column(String, default="enrolled")
    withdrawn = Column(Boolean, default=False)
    screening = relationship("Screening", back_populates="subject")


class EnrolmentHold(Base):
    """A study-wide stop on new enrolments, set by an urgent safety measure."""

    __tablename__ = "enrolment_holds"
    study_id = Column(String, primary_key=True)
    reason = Column(String, nullable=False)
    since = Column(DateTime(timezone=True), nullable=False)
