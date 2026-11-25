resource "aws_sns_topic" "audit_events" {
  name = "audit-events"
}

data "aws_sns_topic" "trial_events" {
  name = "trial-events"
}

data "aws_sns_topic" "safety_events" {
  name = "safety-events"
}

resource "aws_sqs_queue" "audit_ingest" {
  name                       = "audit-ingest"
  visibility_timeout_seconds = 60
  message_retention_seconds  = 1209600
}

resource "aws_sns_topic_subscription" "audit" {
  topic_arn = aws_sns_topic.audit_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.audit_ingest.arn
}

resource "aws_sns_topic_subscription" "trial" {
  topic_arn = data.aws_sns_topic.trial_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.audit_ingest.arn
}

resource "aws_sns_topic_subscription" "safety" {
  topic_arn = data.aws_sns_topic.safety_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.audit_ingest.arn
}
