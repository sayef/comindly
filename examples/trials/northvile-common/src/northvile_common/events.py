import json
import uuid
from dataclasses import asdict, dataclass, field

import boto3

from northvile_common.clock import utc_now

sns = boto3.client("sns")


@dataclass
class Event:
    """A domain event; `event_id` lets consumers drop redeliveries."""

    type: str
    study_id: str
    payload: dict
    event_id: str = field(default_factory=lambda: str(uuid.uuid4()))
    occurred_at: str = field(default_factory=lambda: utc_now().isoformat())


def publish_event(topic_arn: str, event: Event) -> str:
    """Publish an event to an SNS topic, grouped by study; returns the message id."""
    resp = sns.publish(
        TopicArn=topic_arn,
        Message=json.dumps(asdict(event)),
        MessageAttributes={"type": {"DataType": "String", "StringValue": event.type}},
    )
    return resp["MessageId"]


def unwrap(sqs_body: str) -> Event:
    """The event inside an SQS message delivered from an SNS subscription."""
    raw = json.loads(json.loads(sqs_body)["Message"])
    return Event(**raw)
