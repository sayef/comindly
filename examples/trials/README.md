# Northvile Clinical: an example clinical-trials platform

Repositories of a made-up research organisation that runs clinical studies, used for comindly's demo
and screenshots. They talk to each other over HTTP, SNS topics, SQS queues, S3 buckets and shared
tables, the way a real platform does. The code is illustrative and does not run as a product.

| Repository | Language | What it does |
|---|---|---|
| `trial-registry` | Java (Spring) | Studies, protocol versions and amendments, sites, lab ranges; publishes `protocol-amendments` |
| `site-portal` | TypeScript | The site staff front end: screening, consent, enrolment, visits, forms, adverse events |
| `enrolment` | Python (FastAPI) | Screening and eligibility, consent check, randomisation call; publishes `trial-events` |
| `randomisation` | Go | Stratified block randomisation; serves only the blinded kit code, unblinds for safety |
| `consent` | Kotlin (Spring) | e-Consent forms per protocol version, signatures, re-consent after amendments |
| `visit-scheduler` | Python (FastAPI) | Visit windows from the protocol schedule; consumes `visit-scheduling` and `visit-protocol-changes` |
| `lab-ingest` | Rust (axum) | Reads vendor lab files from S3, normalises units, grades results, publishes `lab-flags` |
| `safety` | Python (FastAPI) | Adverse events, seriousness and expectedness, SUSAR clocks, unblinded reports; publishes `safety-events` |
| `edc` | Java (Spring) | Case report forms, edit checks, queries to sites, form and database locks |
| `audit-trail` | Go | Append-only, hash-chained audit log; consumes `audit-ingest`, serves entity history |
| `northvile-common` | Python package | HTTP client, events, audit and clock helpers; `visit-scheduler` still pins an older version |
| `northvile-jvm-common` | Java library | Audit client, study ids, UTC clock and blinding rules for the JVM services |

Index them with `comindly link examples/trials/*`.
