import boto3
from service_a.cfg import config

s3 = boto3.client("s3")


def store(key: str, data: bytes) -> None:
    s3.put_object(Bucket=config.aws.uploads_bucket, Key=key, Body=data)
