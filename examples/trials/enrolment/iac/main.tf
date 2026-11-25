resource "aws_sns_topic" "trial_events" {
  name = "trial-events"
}

data "aws_sns_topic" "safety_events" {
  name = "safety-events"
}

resource "aws_sqs_queue" "safety_holds" {
  name                       = "enrolment-safety-holds"
  visibility_timeout_seconds = 30
}

resource "aws_sns_topic_subscription" "safety_holds" {
  topic_arn     = data.aws_sns_topic.safety_events.arn
  protocol      = "sqs"
  endpoint      = aws_sqs_queue.safety_holds.arn
  filter_policy = jsonencode({ type = ["urgent_safety_measure.raised"] })
}
