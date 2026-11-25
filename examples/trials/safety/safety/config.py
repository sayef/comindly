from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    randomisation_url: str = "http://randomisation"
    lab_ingest_url: str = "http://lab-ingest"
    enrolment_url: str = "http://enrolment"
    safety_events_topic_arn: str = "arn:aws:sns:eu-central-1:123456789012:safety-events"
    lab_flags_queue_url: str = "https://sqs.eu-central-1.amazonaws.com/123456789012/safety-lab-flags"
    reports_bucket: str = "northvile-safety-reports"
    lab_drops_bucket: str = "northvile-prod-lab-drops"


settings = Settings()
