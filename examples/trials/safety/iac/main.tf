resource "aws_sns_topic" "safety_events" {
  name = "safety-events"
}

data "aws_sns_topic" "lab_flags" {
  name = "lab-flags"
}

resource "aws_sqs_queue" "lab_flags" {
  name                       = "safety-lab-flags"
  visibility_timeout_seconds = 60
}

resource "aws_sns_topic_subscription" "lab_flags" {
  topic_arn = data.aws_sns_topic.lab_flags.arn
  protocol  = "sqs"
  endpoint  = aws_sqs_queue.lab_flags.arn
}

resource "aws_s3_bucket" "reports" {
  bucket = "northvile-safety-reports"
}
