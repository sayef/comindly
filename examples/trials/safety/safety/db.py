from sqlalchemy import Boolean, Column, Date, DateTime, ForeignKey, Integer, String, Text
from sqlalchemy.orm import declarative_base, sessionmaker

Base = declarative_base()
SessionLocal = sessionmaker()


class AdverseEvent(Base):
    __tablename__ = "adverse_events"
    id = Column(String, primary_key=True)
    study_id = Column(String, nullable=False)
    subject_id = Column(String, nullable=False, index=True)
    site_id = Column(String, nullable=False)
    term = Column(String, nullable=False)
    onset_date = Column(Date, nullable=False)
    severity_grade = Column(Integer)
    serious = Column(Boolean, default=False)
    seriousness_criteria = Column(String)
    related = Column(Boolean)
    expected = Column(Boolean)
    aware_at = Column(DateTime(timezone=True), nullable=False)
    source = Column(String, default="site")
    narrative = Column(Text)


class SafetyReport(Base):
    """An expedited report to the authorities, e.g. a SUSAR, with its regulatory clock."""

    __tablename__ = "safety_reports"
    id = Column(String, primary_key=True)
    adverse_event_id = Column(String, ForeignKey("adverse_events.id"), nullable=False)
    kind = Column(String, nullable=False)
    due_at = Column(DateTime(timezone=True), nullable=False)
    submitted_at = Column(DateTime(timezone=True))
    document_key = Column(String)
