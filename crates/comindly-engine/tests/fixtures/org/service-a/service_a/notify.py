from acme.common.queues import FairQueue, publish

EVENTS_TOPIC = "arn:aws:sns:eu-central-1:123456789012:user-events"


def notify(user_id: str) -> None:
    publish(EVENTS_TOPIC, user_id)


def enqueue(user_id: str) -> None:
    q = FairQueue(queue_url="https://sqs.eu-central-1.amazonaws.com/123456789012/user-jobs")
    q.send(user_id)
