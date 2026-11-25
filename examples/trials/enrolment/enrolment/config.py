import os

RANDOMISATION_URL = os.environ.get("RANDOMISATION_URL", "http://randomisation")
CONSENT_URL = os.environ.get("CONSENT_URL", "http://consent")
TRIAL_EVENTS_TOPIC = "arn:aws:sns:eu-central-1:123456789012:trial-events"
SAFETY_HOLDS_QUEUE = "https://sqs.eu-central-1.amazonaws.com/123456789012/enrolment-safety-holds"
