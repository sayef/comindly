from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    registry_url: str = "http://trial-registry"
    scheduling_queue_url: str = "https://sqs.eu-central-1.amazonaws.com/123456789012/visit-scheduling"
    protocol_changes_queue_url: str = "https://sqs.eu-central-1.amazonaws.com/123456789012/visit-protocol-changes"
    reminders_queue_url: str = "https://sqs.eu-central-1.amazonaws.com/123456789012/visit-reminders"
    reminder_days_ahead: int = 3


settings = Settings()
