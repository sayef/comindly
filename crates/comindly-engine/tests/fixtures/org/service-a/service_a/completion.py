import asyncio

import aioboto3


async def publish_to_topic(topic, message: str) -> None:
    arn = await asyncio.to_thread(getattr, topic, "arn")
    params = dict(TopicArn=arn, Message=message)
    async with aioboto3.Session().client("sns") as sns:
        await sns.publish(**params)


class CompletionPublisher:
    def __init__(self, sns_topic):
        self._topic = sns_topic

    async def __call__(self, message: str) -> None:
        await publish_to_topic(self._topic, message)


def build(config):
    return CompletionPublisher(config.completion.sns_topic)
