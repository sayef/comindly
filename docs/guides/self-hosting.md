# Self-hosting

comindly runs as a server for a team from one container image: members sign in by email or with
codes, admins invite people and manage the tokens it uses, and the index lives in your
[storage](storage.md). Agents connect to it over HTTP. Docker, Kubernetes, ECS, Cloud Run or a VM
all work the same way, with the image `ghcr.io/sayef/comindly` (amd64 and arm64, a tag per release).

## Settings

The server reads its settings from its environment once, when it starts, and refuses to start with a
message when one is missing or wrong. Each secret can instead be a file named by the
same variable with `_FILE` (Docker and Kubernetes secrets, Vault, AWS Secrets Manager or Google
Secret Manager mounts), so rotating one is a change in your secret store and a restart.

| Variable | Required | Purpose |
| --- | --- | --- |
| `COMINDLY_PUBLIC_URL` | yes | The address members open, e.g. `https://comindly.example.com`. Setting it turns server mode on. |
| `COMINDLY_ADMIN_EMAILS` | yes | Comma-separated admins. Always admins; they cannot be removed or demoted in the app. |
| `COMINDLY_ADMIN_TOKEN` | recommended | Your own key, at least 32 random characters: API access as the first admin, and sign-in when email is unavailable. |
| `COMINDLY_SECRET_KEY` | yes | Encrypts tokens and keys saved in the app and signs every account record, so records added to the bucket by anyone else are ignored. Keep it in your secret store. |
| `COMINDLY_SECRET_KEY_PREVIOUS` | when rotating | The old key: on start, stored secrets and records are re-encrypted and re-signed with the new one. |
| `COMINDLY_SMTP_URL` | for email sign-in | Mail server, e.g. `smtps://user:password@smtp.example.com`. Without it, members sign in with codes. |
| `COMINDLY_SIGN_IN` | optional | `email` or `code`; by default email when `COMINDLY_SMTP_URL` is set, else code. |
| `COMINDLY_SMTP_FROM` | with SMTP | Sender, e.g. `comindly <comindly@example.com>`. |
| `COMINDLY_INDEX_DIR` | yes | Where the index lives (see [Storage](storage.md)). |
| `GH_TOKEN`, `GITLAB_TOKEN` | optional | Repository access, if you keep it with the deployment rather than in the app. `GH_TOKEN` is used for github.com, `GITLAB_TOKEN` for gitlab.com. |
| `COMINDLY_GITLAB_HOSTS` | for self-managed GitLab | Comma-separated hosts that also get `GITLAB_TOKEN`, e.g. `gitlab.example.com`. |
| `OPENAI_API_KEY` | optional | LLM key, likewise; `COMINDLY_LLM_BASE_URL` for any OpenAI-compatible endpoint. |
| `COMINDLY_LOG_FORMAT` | the image sets it | `json`: one JSON event per line (timestamp, level, step, message, counts, tokens, cost) for Datadog, CloudWatch or Loki. Unset: readable lines. |
| `COMINDLY_TRUST_PROXY` | behind a proxy | `true` when a proxy you run sets `X-Forwarded-For`, so sign-in limits apply per client. |

Members, sessions, personal tokens, stored secrets, the repositories the server indexes and the record
of each indexing job live next to the index, in `_server` at the index location, so a container keeps
nothing it needs on its own disk. The index's shared settings (models, LLM steps, limits) live with the
index itself. A server keeps no configuration file. Token values are never
stored, only their hashes; saved git and LLM keys are encrypted with `COMINDLY_SECRET_KEY`. Email
addresses, roles and token names are readable to anyone who can read the bucket, like the index.

## Signing in

A server signs everyone in one way: by email, or by codes. The deployment decides.

**By email** (with `COMINDLY_SMTP_URL`): invitations are emailed links for one address that work once
within 7 days; to sign in, members ask for a link that works once, for 15 minutes. Opening a link asks
for a click, so mail scanners cannot use it up.

**By code**: an admin invites an address and hands over the invitation code it gets (once, for that
address, within 7 days). With it, the person signs in for the first time and receives their own
sign-in code, shown once, which they use with their email address from then on. They can make a new
one under Settings → Your access; an admin can reset it for someone who lost theirs. Each new code
signs the person out everywhere else. Codes are generated, never chosen; five wrong ones pause that
address for 15 minutes from the same client only, so nobody can lock someone else out.

**In both**: admins named in `COMINDLY_ADMIN_EMAILS` can always sign in with the admin token too;
sessions last 30 days; removing a member ends their sessions, codes and tokens.

## Roles

| Role | Can |
| --- | --- |
| Viewer | Read everything: the app, the API and agents' tools. Keeps their own access tokens. |
| Member | Also add to the ledger and the conventions, in the app, from the command line and through agents. |
| Admin | Also invite, change and remove people, and manage repositories, settings, the server's tokens and indexing. |

Admins named in `COMINDLY_ADMIN_EMAILS` are always admins. An agent works with the role of the person
whose token it uses.

![Settings → Members on a server](../assets/screenshots/members.webp#only-light)
![Settings → Members on a server](../assets/screenshots/members-dark.webp#only-dark)

## Agents

Agents reach the server's tools at `https://<your server>/mcp` (MCP over HTTP) with a personal
access token from Settings → Your access, which also shows the command or config for Claude Code,
Cursor, Codex and VS Code. For Claude Code:

```sh
claude mcp add --transport http --scope user comindly https://comindly.example.com/mcp \
  --header "Authorization: Bearer <token>"
```

![Settings → Your access: a sign-in code, agent tokens and the setup for each agent](../assets/screenshots/access.webp#only-light)
![Settings → Your access: a sign-in code, agent tokens and the setup for each agent](../assets/screenshots/access-dark.webp#only-dark)

Until the first index exists, agents get `503` with the reason; they connect once an admin has
indexed. Nothing is installed on the developer's machine, and no storage credentials leave the server. A
personal access token is for agents and scripts: it reads and writes the index and the ledger, but
cannot manage the account, its tokens, members or the server's secrets; that needs signing in. It can
revoke itself (`POST /api/v1/auth/token/revoke`), as `comindly logout` does.
A token lasts 30 days, 90 days (the default), a year or without expiry, chosen when it is created.
An expired token stops working and shows as expired under Settings → Your access; it is removed 30
days later. Through the API, `POST /api/v1/tokens` takes `{"name": "…", "expires_in_days": 90}`
(`null` for no expiry). Records an agent writes say which member it works for; after each re-index, agents get the new
index without reconnecting. Ledger records written in the app carry the member's address.

## The command line

Developers point their `comindly` command at the server:

```sh
comindly login https://comindly.example.com
```

It prints an address and a short code and opens the browser; once signed in there, the member checks
the code and approves. The command line then has its own personal access token, named after the
machine, and `find`, `search`, `explore`, `why`, `ledger`, `flow`, `stats`, `contracts`,
`packages`, `conventions`, `preflight`, `remember`, `review` and `link` ask the server
([Command line](../reference/cli.md#against-a-server)). Admins can start indexing with `comindly link`
and follow it in the terminal. `comindly doctor` shows who the command line is signed in as and
whether its token still works. `comindly logout` revokes that token on the server and forgets the
sign-in on that machine; when the server cannot be reached, it says so, and the token can be revoked
under Settings → Your access. The command line's token expires after 90 days; the command line then
says so and asks to run `comindly login` again.

![Approving a command line's sign-in](../assets/screenshots/device.webp#only-light)
![Approving a command line's sign-in](../assets/screenshots/device-dark.webp#only-dark)

## Tokens and keys

Settings → Integrations shows each token the server uses and where it comes from, never its value.
One the deployment provides wins and is read-only there; otherwise an admin pastes one, and it is
stored encrypted with `COMINDLY_SECRET_KEY`. Read-only tokens are enough: a GitLab group access token
with `read_repository` and `read_api`, or a fine-grained GitHub token with Contents and Metadata read.

## Audit log

Admins see who changed what under Settings → Audit, newest first: sign-ins and sign-outs, joining,
invitations, role changes, removals, sign-in code resets, tokens created and revoked, command-line
sign-ins approved or denied, secrets saved or removed, and changes to the repositories. Token values and
codes are never recorded. The log is also at `GET /api/v1/audit` (admins, signed in): `limit`
(default 50, at most 500), `before` (the previous page's `next`), and optional `actor` and `action`
filters. Its rows are signed like the rest of the accounts.

## Deploying

The image runs `comindly server`, which listens on port 7878 on all interfaces
(`--port` and `--host` change that) and refuses to start without `COMINDLY_PUBLIC_URL`. Give it its
settings and secrets, access to the index location, and HTTPS in front. Without signing in, `GET /health` answers `200` while the process serves requests,
and `GET /ready` answers `200` once an index is loaded and the accounts can be read (`503` with the
reason until then).

Indexing runs inside the server as a job, one at a time; starting one while another runs queues it
(up to 5, and asking again for the same thing returns the job already waiting). On SIGTERM the server
stops taking requests, lets a running job stop at its next checkpoint and saves its record; a job cut
off by a crash shows as interrupted after the restart, and queued jobs that never ran are marked
interrupted too, to be started again. Summaries and walkthroughs already written are kept either way, so the next run continues
where it stopped. It keeps clones of repositories under `/data`; a volume there saves cloning
them again after a restart but is not required.

### Docker

```sh
docker run -d --name comindly -p 127.0.0.1:7878:7878 \
  -e COMINDLY_PUBLIC_URL=https://comindly.example.com \
  -e COMINDLY_ADMIN_EMAILS=you@example.com \
  -e COMINDLY_INDEX_DIR=s3://bucket/comindly \
  -e COMINDLY_ADMIN_TOKEN_FILE=/run/secrets/admin_token \
  -e COMINDLY_SECRET_KEY_FILE=/run/secrets/secret_key \
  -v /etc/comindly/secrets:/run/secrets:ro \
  ghcr.io/sayef/comindly:latest
```

Pin a version tag (`ghcr.io/sayef/comindly:0.4.0`) for anything you rely on. Generate the two
secrets once, for example with `openssl rand -base64 32`, and keep them in your secret store.

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata: { name: comindly }
spec:
  replicas: 1   # indexing runs inside this one instance
  selector: { matchLabels: { app: comindly } }
  template:
    metadata: { labels: { app: comindly } }
    spec:
      serviceAccountName: comindly   # its cloud role reads the bucket
      containers:
        - name: comindly
          image: ghcr.io/sayef/comindly:0.4.0
          ports: [{ containerPort: 7878 }]
          env:
            - { name: COMINDLY_PUBLIC_URL, value: https://comindly.example.com }
            - { name: COMINDLY_ADMIN_EMAILS, value: you@example.com }
            - { name: COMINDLY_INDEX_DIR, value: s3://bucket/comindly }
            - { name: COMINDLY_TRUST_PROXY, value: "true" }
            - { name: COMINDLY_ADMIN_TOKEN_FILE, value: /run/secrets/admin_token }
            - { name: COMINDLY_SECRET_KEY_FILE, value: /run/secrets/secret_key }
          volumeMounts:
            - { name: data, mountPath: /data }   # clones of repositories; scratch is fine
            - { name: secrets, mountPath: /run/secrets, readOnly: true }
          readinessProbe: { httpGet: { path: /ready, port: 7878 } }
          livenessProbe: { httpGet: { path: /health, port: 7878 } }
      volumes:
        - name: data
          emptyDir: {}
        - name: secrets
          secret: { secretName: comindly }
---
apiVersion: v1
kind: Service
metadata: { name: comindly }
spec:
  selector: { app: comindly }
  ports: [{ port: 80, targetPort: 7878 }]
```

Route your ingress to the service with TLS. On ECS, Cloud Run or Azure Container Apps the same
applies: the image, the variables above, secrets from the platform's store, and the task's or
workload's role for the bucket.

### Behind a proxy

Terminate TLS at your proxy or load balancer and forward to port 7878, keeping the original `Host`
header (agents' requests to `/mcp` are checked against it). Set `COMINDLY_PUBLIC_URL` to the
`https://` address people open (sign-in cookies are then secure and host-only, and links in emails
point there) and `COMINDLY_TRUST_PROXY=true` so sign-in limits apply per client, using the address
your proxy appends to `X-Forwarded-For`.

### Backups and upgrades

Back up the index location (bucket versioning works); it holds the index, the ledger and the
accounts. To upgrade, start the new image tag with the same settings. A server upgraded from 0.3 moves
the settings it kept from its configuration file into its database and the index on first start.

## API

The app and the command line use the same JSON API under `/api/v1`, signed in with the session cookie
or a personal access token (`Authorization: Bearer <token>`). Errors have one shape,
`{"error": {"code": "…", "message": "…"}}`, with an HTTP status that matches. Indexing jobs are under
`/api/v1/jobs`: their status (a queued one with its `position`), their events, and
`/api/v1/jobs/{id}/events` streams a job's events as server-sent events, waiting while it is queued.
`POST /api/v1/index` answers with the job's `id`, its `status` (`running` or `queued`) and `position`;
`POST /api/v1/jobs/{id}/cancel` stops a running job or takes a queued one out of the queue.

## Trying it locally

```sh
scripts/docker-local.sh      # builds the Linux binary here, packages comindly:local, makes .secrets/
docker compose up
```

Open <http://localhost:7878>, sign in as `admin@example.com` and read the link at
<http://localhost:8025> (Mailpit). `COMINDLY_SIGN_IN=code docker compose up -d` switches to codes;
the admin token for the first sign-in is in `.secrets/admin_token`. The index goes to a local S3-compatible store (SeaweedFS).
`docker compose down -v` removes everything.
