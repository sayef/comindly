locals {
  team = "acme"
}

data "aws_sns_topic" "users" {
  name = "user-events"
}

module "jobs_queue" {
  source     = "git::https://example.com/modules//queue"
  team       = local.team
  tier       = var.tier
  queue_name = "user-jobs"

  sns_subscriptions = {
    users = { topic_arn = data.aws_sns_topic.users.arn }
  }
}
