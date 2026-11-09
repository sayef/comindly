import os

import boto3

sns = boto3.client("sns")
COURSE_TOPIC = os.environ["COURSE_EVENTS_TOPIC_ARN"]


def publish_course_updated(course_id: str):
    sns.publish(TopicArn=COURSE_TOPIC, Message=course_id)
