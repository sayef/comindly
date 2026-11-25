variable "tier" {
  type = string
}

resource "aws_s3_bucket" "lab_drops" {
  bucket = "northvile-${var.tier}-lab-drops"
}

resource "aws_sns_topic" "lab_flags" {
  name = "lab-flags"
}
