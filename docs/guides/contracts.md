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
| SQS / SNS / S3 | boto3 publishers and writers, also through wrappers and constructors | boto3 subscribers and readers |
| SQL tables | SQLAlchemy models, pypika, raw SQL writes | reads through the same |
| Infrastructure | Terraform `aws_sqs_queue`, `aws_sns_topic`, `aws_s3_bucket`, team modules (`queue_name`, `topic_name`, `bucket_name`) | topic subscriptions (`aws_sns_topic_subscription`, `*topic_arn` inputs) |

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
