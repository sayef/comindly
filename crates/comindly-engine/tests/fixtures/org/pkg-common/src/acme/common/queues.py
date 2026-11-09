import boto3


class Queue:
    def __init__(self, queue_url: str):
        self._queue_url = queue_url

    def _sqs(self):
        return boto3.client("sqs")

    def send(self, body: str) -> None:
        client = self._sqs()
        client.send_message(QueueUrl=self._queue_url, MessageBody=body)


class FairQueue(Queue):
    def send(self, body: str) -> None:
        client = self._sqs()
        client.send_message(QueueUrl=self._queue_url, MessageBody=body, MessageGroupId="g")


def make_client():
    return boto3.client("sns")


def publish(topic_arn: str, message: str) -> None:
    sns = make_client()
    sns.publish(TopicArn=topic_arn, Message=message)
