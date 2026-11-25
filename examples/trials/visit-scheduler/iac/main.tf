data "aws_sns_topic" "trial_events" {
  name = "trial-events"
}

data "aws_sns_topic" "protocol_amendments" {
  name = "protocol-amendments"
}

resource "aws_sqs_queue" "scheduling" {
  name                       = "visit-scheduling"
  visibility_timeout_seconds = 60
}

resource "aws_sns_topic_subscription" "scheduling" {
  topic_arn = data.aws_sns_topic.trial_events.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.scheduling.arn
}

resource "aws_sqs_queue" "protocol_changes" {
  name                       = "visit-protocol-changes"
  visibility_timeout_seconds = 300
}

resource "aws_sns_topic_subscription" "protocol_changes" {
  topic_arn = data.aws_sns_topic.protocol_amendments.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.protocol_changes.arn
}

resource "aws_sqs_queue" "reminders" {
  name = "visit-reminders"
}
