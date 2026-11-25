from sqlalchemy import Column, Date, DateTime, String
from sqlalchemy.orm import declarative_base, sessionmaker

Base = declarative_base()
SessionLocal = sessionmaker()


class Visit(Base):
    """A planned or completed visit. Dates are calendar dates in UTC, never site-local."""

    __tablename__ = "visits"
    id = Column(String, primary_key=True)
    subject_id = Column(String, nullable=False, index=True)
    study_id = Column(String, nullable=False)
    code = Column(String, nullable=False)
    protocol_version = Column(String, nullable=False)
    target_date = Column(Date, nullable=False)
    window_start = Column(Date, nullable=False)
    window_end = Column(Date, nullable=False)
    status = Column(String, default="planned")
    completed_at = Column(DateTime(timezone=True))


class ProcessedEvent(Base):
    """Event ids already handled, so SQS redeliveries do not create duplicate visit plans."""

    __tablename__ = "processed_events"
    event_id = Column(String, primary_key=True)
    processed_at = Column(DateTime(timezone=True), nullable=False)
