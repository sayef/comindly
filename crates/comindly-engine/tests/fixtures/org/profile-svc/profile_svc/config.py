from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    courses_api_url: str = "http://courses-svc"
    exports_bucket: str = "acme-profile-exports"


settings = Settings()
