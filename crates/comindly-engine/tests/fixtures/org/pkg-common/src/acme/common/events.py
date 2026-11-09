import boto3

_sqs = boto3.client("sqs")


def publish(queue_url, body):
    _sqs.send_message(QueueUrl=queue_url, MessageBody=body)
