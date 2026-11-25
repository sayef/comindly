# Service contracts

A contract is a route, queue, topic, bucket or SQL table. comindly finds who provides it and who uses it,
across repositories, so blast radius crosses service boundaries.

```sh
comindly contracts                                 # everything
comindly contracts --type http --dangling          # routes nobody calls, calls nobody serves
```

## What is detected

| Kind | Providers | Consumers |
|---|---|---|
| HTTP | FastAPI, Next.js route handlers, Express, Spring MVC, Go net/http, gin, echo, chi, axum, actix | httpx, requests, fetch, axios (and client wrappers), Feign, RestTemplate, WebClient, Go `http`, reqwest |
| SQS / SNS / S3 | AWS SDK calls in every supported language (boto3, Go, Java, Kotlin, JavaScript/TypeScript v2 and v3, Rust), Spring Cloud AWS templates; also through wrappers and constructors | the same SDKs' receive and read calls |
| SQL tables | SQLAlchemy models, pypika, and any SQL statement passed to a call, in every language | reads through the same |
| Infrastructure | Terraform `aws_sqs_queue`, `aws_sns_topic`, `aws_s3_bucket`, team modules (`queue_name`, `topic_name`, `bucket_name`) | topic subscriptions (`aws_sns_topic_subscription`, `*topic_arn` inputs) |

The AWS SDKs share their operation and field names, so one table covers all of them: `SendMessage`,
`sendMessage` and `send_message` are the same operation, and `QueueUrl`, `queueUrl` and `queue_url` the
same field, whether it is a keyword argument, a struct field (Go), a builder call (Java, Rust), a Kotlin
block or a command object (JavaScript v3). The service comes from the client's declared type
(`SqsClient`, `sqs.Client`, `SnsTemplate`) or from the field names. Other brokers (Kafka, RabbitMQ) are
not detected yet.

Routes join clients through a base-URL suffix when the client's base path is unknown. Test code is left out:
contracts describe production topology.

A FastAPI router can be a subclass of `APIRouter` (`class ScopedRouter(APIRouter)`, used as
`router = ScopedRouter(prefix="/orders")`); its routes are served like those of `APIRouter`, also when
the subclass lives in another indexed repository. The subclass's name must end in `Router`.

## Where values come from

Paths, queue URLs, topic ARNs and bucket names are evaluated statically: literals, constants, string
templates, `Path` joins, settings defaults, environment variables, function parameters passed by callers
(up to four calls deep), constructor arguments stored on `self`, and finally the repository's YAML config
(`config*`, `settings*`, `application*.yml`). Deployment tiers are normalised (`courses-dev-jobs` and
`courses-${var.tier}-jobs` are the same queue), so dev config, Terraform templates and other tiers meet.
