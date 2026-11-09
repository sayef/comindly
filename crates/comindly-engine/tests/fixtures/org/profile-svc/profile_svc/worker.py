import boto3

from profile_svc.config import settings

sqs = boto3.client("sqs")
s3 = boto3.client("s3")
QUEUE_URL = "https://sqs.eu-central-1.amazonaws.com/123456789012/course-updates"


def poll():
    resp = sqs.receive_message(QueueUrl=QUEUE_URL)
    for msg in resp.get("Messages", []):
        s3.put_object(Bucket=settings.exports_bucket, Key=msg["MessageId"], Body=msg["Body"])
