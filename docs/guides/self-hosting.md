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
| `COMINDLY_PUBLIC_URL` | yes | The address members open, e.g. `https://comindly.example.com`. Setting it makes `comindly-engine` a team server; without it, the engine serves only this machine. |
| `COMINDLY_ADMIN_EMAILS` | yes | Comma-separated admins. Always admins; they cannot be removed or demoted in the app. |
| `COMINDLY_ADMIN_TOKEN` | recommended | Your own key, at least 32 random characters: API access as the first admin, and sign-in when email is unavailable. |
| `COMINDLY_SECRET_KEY` | yes | Encrypts tokens and keys saved in the app and signs every account record, so records added to the bucket by anyone else are ignored. Keep it in your secret store. |
| `COMINDLY_SECRET_KEY_PREVIOUS` | when rotating | The old key: on start, stored secrets and records are re-encrypted and re-signed with the new one. |
| `COMINDLY_SMTP_URL` | for email sign-in | Mail server, e.g. `smtps://user:password@smtp.example.com`. Without it, members sign in with codes. |
| `COMINDLY_SIGN_IN` | optional | The ways to sign in, comma-separated: `oidc`, `email`, `code` (e.g. `oidc,email,code`). By default the provider when `COMINDLY_OIDC_ISSUER` is set and email when `COMINDLY_SMTP_URL` is set, else codes. |
| `COMINDLY_OIDC_ISSUER` | for provider sign-in | The OpenID Connect issuer, e.g. `https://login.microsoftonline.com/<tenant id>/v2.0`. Setting it turns provider sign-in on. |
| `COMINDLY_OIDC_CLIENT_ID`, `COMINDLY_OIDC_CLIENT_SECRET` | with the issuer | The app registered at the provider. Keep the secret in your secret store (`COMINDLY_OIDC_CLIENT_SECRET_FILE` reads it from a file). |
| `COMINDLY_OIDC_LABEL` | optional | The provider's name on the sign-in button, e.g. `Microsoft`. |
| `COMINDLY_OIDC_ALLOWED_DOMAINS` | optional | Comma-separated email domains whose people join as viewers on first sign-in, without an invitation. |
| `COMINDLY_SMTP_FROM` | with SMTP | Sender, e.g. `comindly <comindly@example.com>`. |
| `COMINDLY_INDEX_DIR` | yes | Where the index lives (see [Storage](storage.md)). |
| `GH_TOKEN`, `GITLAB_TOKEN` | optional | Repository access, if you keep it with the deployment rather than in the app. `GH_TOKEN` is used for github.com, `GITLAB_TOKEN` for gitlab.com. |
| `COMINDLY_GITLAB_HOSTS` | for self-managed GitLab | Comma-separated hosts that also get `GITLAB_TOKEN`, e.g. `gitlab.example.com`. |
| `OPENAI_API_KEY` | optional | LLM key, likewise; `COMINDLY_LLM_BASE_URL` for any OpenAI-compatible endpoint. |
| `COMINDLY_LOG_FORMAT` | the image sets it | `json`: one JSON event per line (timestamp, level, kind, step, message, `job_id`, `request_id`, counts, tokens, cost) for Datadog, CloudWatch or Loki. Unset: readable lines. |
| `COMINDLY_LOG_LEVEL` | optional | `error`, `warn`, `info` (default) or `debug`. `debug` adds access lines for `/health` and `/ready`. |
| `COMINDLY_LOG_SKIP` | optional | Kinds of lines not to log, comma-separated: `access` (one per request) and `tool` (one per MCP tool call, with the client's name). Indexing, warnings and errors stay. Admins switch both in Settings → General → Logging, which takes effect at once and is kept. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | optional | Send traces over OTLP/HTTP (protobuf) to this collector, e.g. `http://otel-collector:4318`: a span per request (request id, method, route, status) and per indexing job (job id, the request that started it). Unset: no export. `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` and `OTEL_EXPORTER_OTLP_HEADERS` work as usual. Built into the container image and the Linux release binaries; a build from source needs `--features otel`. An unreachable collector delays shutdown by at most 2 s. |
| `OTEL_SERVICE_NAME` | optional | The service name on exported traces; `comindly` by default. |
| `COMINDLY_TRUST_PROXY` | behind a proxy | `true` when a proxy you run sets `X-Forwarded-For`, so sign-in limits apply per client. |
| `COMINDLY_GATEWAY_SECRET` | no | At least 32 random characters, shared with a gateway you run in front of `/mcp`; see [Agents through a gateway](#agents-through-a-gateway). Off when unset. |
| `COMINDLY_GATEWAY_EMAIL_HEADER` | no | The header your gateway names the signed-in member in; `X-Forwarded-Email` by default. |
| `COMINDLY_GATEWAY_URL` | no | The gateway's MCP address, e.g. `https://mcp.example.com/mcp`. Your access then shows members how to connect through it, with no token. |

Members, sessions, personal tokens, stored secrets, the repositories the server indexes and the record
of each indexing job live next to the index, in `_server` at the index location, so a container keeps
nothing it needs on its own disk. Its clones are only a working copy for indexing: the index stores the
contents of the files it indexed, so code is shown from there, and a restarted container clones again
on its next index run. Anyone who can read the bucket can therefore read that code. The index's shared settings (models, LLM steps, limits) live with the
index itself. A server keeps no configuration file. Token values are never
stored, only their hashes; saved git and LLM keys are encrypted with `COMINDLY_SECRET_KEY`. Email
addresses, roles and token names are readable to anyone who can read the bucket, like the index.

## Signing in

A server offers one or more ways to sign in, all for invited people only: through your identity
provider, by email, and by codes. The deployment decides with `COMINDLY_SIGN_IN`, and the sign-in page
shows each one it offers. Invitations are emailed when the server sends mail, else they are a code to
hand over; an invited person can also join through the provider with that address.

**Through your identity provider** (with `COMINDLY_OIDC_ISSUER`): the sign-in page shows one button,
"Continue with" the provider, which signs people in with their company account (OpenID Connect,
authorization code flow with PKCE). The provider checks passwords and MFA; comindly checks the ID
token's signature, issuer, audience, expiry and nonce, and takes its `email` claim (or
`preferred_username` when that is an address). Who may get in stays in comindly: members sign in,
invited addresses join with the invitation's role, and with `COMINDLY_OIDC_ALLOWED_DOMAINS` anyone at
those domains joins as a viewer. Everyone else is turned away. Register comindly at the provider as a
web app with the redirect URI `<COMINDLY_PUBLIC_URL>/auth/oidc/callback` and the scopes `openid email
profile`:

- **Microsoft Entra ID**: App registrations → New registration → Web, that redirect URI; then
  Certificates & secrets → New client secret. The issuer is
  `https://login.microsoftonline.com/<tenant id>/v2.0`. Add the optional `email` claim to the ID
  token, or rely on `preferred_username`.
- **Google Workspace**: Google Cloud console → Credentials → OAuth client ID → Web application. The
  issuer is `https://accounts.google.com`.
- **Okta, Auth0, Keycloak, Cognito**: a confidential web client with that redirect URI; the issuer is
  the address its `/.well-known/openid-configuration` lives under.

**By email** (with `COMINDLY_SMTP_URL`): invitations are emailed links for one address that work once
within 7 days; to sign in, members ask for a link that works once, for 15 minutes. Opening a link asks
for a click, so mail scanners cannot use it up.

**By code**: an admin invites an address and hands over the invitation code it gets (once, for that
address, within 7 days), or gives an existing member a code with Reset code. With it, the person signs in for the first time and receives their own
sign-in code, shown once, which they use with their email address from then on. They can make a new
one under Your access; an admin can reset it for someone who lost theirs. Each new code
signs the person out everywhere else. Codes are generated, never chosen; five wrong ones pause that
address for 15 minutes from the same client only, so nobody can lock someone else out.

**In every mode**: admins named in `COMINDLY_ADMIN_EMAILS` can always sign in with the admin token too;
sessions last 30 days.

**Offboarding**: remove the person under Settings → Members. That deletes their sessions, personal
access tokens, sign-in code, invitations and pending sign-in links at once, so every way back in closes
together, the provider included. People named in `COMINDLY_ADMIN_EMAILS` are removed by changing that
setting. comindly keeps no keys of their own for members; if they knew the server's git or LLM tokens,
rotate those under Settings → Integrations.

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
access token from Your access, which also shows the command or config for Claude Code,
Cursor, Codex and VS Code. For Claude Code:

```sh
claude mcp add --transport http --scope user comindly https://comindly.example.com/mcp \
  --header "Authorization: Bearer <token>"
```

![Your access: a sign-in code, agent tokens and the setup for each agent](../assets/screenshots/access.webp#only-light)
![Your access: a sign-in code, agent tokens and the setup for each agent](../assets/screenshots/access-dark.webp#only-dark)

Until the first index exists, agents get `503` with the reason; they connect once an admin has
indexed. Nothing is installed on the developer's machine, and no storage credentials leave the server. A
personal access token is for agents and scripts: it reads and writes the index and the ledger, but
cannot manage the account, its tokens, members or the server's secrets; that needs signing in. It can
revoke itself (`POST /api/v1/auth/token/revoke`), as `comindly logout` does.
A token lasts 30 days, 90 days (the default), a year or without expiry, chosen when it is created.
An expired token stops working and shows as expired under Your access; it is removed 30
days later. Through the API, `POST /api/v1/tokens` takes `{"name": "…", "expires_in_days": 90}`
(`null` for no expiry). Records an agent writes say which member it works for; after each re-index, agents get the new
index without reconnecting. Ledger records written in the app carry the member's address.

### Agents through a gateway

To keep the server private while an outside service (a code review bot, say) reaches `/mcp`, run a
gateway that signs people in and forwards their calls: an MCP OAuth proxy, oauth2-proxy, Pomerium or
similar. Set `COMINDLY_GATEWAY_SECRET`, and have the gateway send it in `X-Gateway-Secret` with the
signed-in person's address in `X-Forwarded-Email` (or the header set in
`COMINDLY_GATEWAY_EMAIL_HEADER`). It may also name the app in `X-Forwarded-Client` ("Claude Code", a
review bot), which tool-call log lines then show. comindly answers only members, each with their own
role; anyone else is refused, and both headers are ignored without the secret. To keep a client such as a
review bot read-only, allow it only the read tools. Only the gateway should be able to reach the
server, and it must drop these headers when a client sends them. Without a gateway, nothing changes:
agents use personal access tokens as above.

Someone invited joins on their first call through the gateway, as they would on signing in to the
app, so they never need to open it. Set `COMINDLY_GATEWAY_URL` and Your access shows the
command for each agent. An MCP OAuth proxy only sends people back to the addresses it allows: local
ones (`http://localhost:*`, `http://127.0.0.1:*`) cover Claude Code, Cursor, Codex and VS Code; the
Claude desktop app and claude.ai need `https://claude.ai/api/mcp/auth_callback` and
`https://claude.com/api/mcp/auth_callback`.

## The command line

Developers add the server to their `comindly` command as a profile:

```sh
comindly use https://comindly.example.com acme
```

It prints an address and a short code and opens the browser; once signed in there, the member checks
the code and approves. The command line then has its own personal access token, named after the
machine, and every command asks the server ([Command line](../reference/cli.md#against-a-server)),
including `comindly serve` for agents and the agent hooks. Admins can start indexing with
`comindly link` and follow it in the terminal. `comindly use local` switches back to the engine on
the developer's machine, and `--use acme` runs one command against the server. `comindly doctor`
shows who the command line is signed in as and whether its token still works. `comindly logout`
revokes that token on the server and forgets the sign-in on that machine; when the server cannot be
reached, it says so, and the token can be revoked under Your access. The command line's
token expires after 90 days; the command line then says so and asks to run `comindly login` again.
Someone signed in with an earlier version, through `comindly login <url>`, finds that sign-in as a
profile called `default`.

![Approving a command line's sign-in](../assets/screenshots/device.webp#only-light)
![Approving a command line's sign-in](../assets/screenshots/device-dark.webp#only-dark)

## Tokens and keys

Settings → Integrations shows each token the server uses and where it comes from, never its value.
An admin pastes one there, and it is stored encrypted with `COMINDLY_SECRET_KEY`. A stored one is
used instead of one the deployment provides; removing it goes back to the deployment's. Read-only tokens are enough: a GitLab group access token
with `read_repository` and `read_api`, or a fine-grained GitHub token with Contents and Metadata read.

## Audit log

Admins see who changed what under Activity → Changes, newest first: sign-ins and sign-outs, joining,
invitations, role changes, removals, sign-in code resets, tokens created and revoked, command-line
sign-ins approved or denied, secrets saved or removed, and changes to the repositories. Token values and
codes are never recorded. The log is also at `GET /api/v1/audit` (admins, signed in): `limit`
(default 50, at most 500), `before` (the previous page's `next`), and optional `actor` and `action`
filters. Its rows are signed like the rest of the accounts.

## Deploying

The image runs `comindly-engine`, which with `COMINDLY_PUBLIC_URL` set is the team server and listens
on port 7878 on all interfaces (`--port` and `--host` change that). Outside the image, start it the
same way:

```sh
COMINDLY_PUBLIC_URL=https://comindly.example.com COMINDLY_ADMIN_EMAILS=you@example.com \
  COMINDLY_INDEX_DIR=s3://bucket/comindly COMINDLY_SECRET_KEY_FILE=/run/secrets/secret_key \
  comindly-engine
```

Without `COMINDLY_PUBLIC_URL` it serves only 127.0.0.1, without sign-in, as the local engine does.
Give it its settings and secrets, access to the index location, and HTTPS in front. Without signing in, `GET /health` answers `200` while the process serves requests,
and `GET /ready` answers `200` once an index is loaded and the accounts can be read (`503` with the
reason until then).

Indexing runs inside the server as a job, one at a time; starting one while another runs queues it
(up to 5, and asking again for the same thing returns the job already waiting). On SIGTERM the server
stops taking requests, lets a running job stop at its next checkpoint and marks it and the queued
jobs paused; once a server is back (the new one, in a rolling or blue/green deploy), it continues them
in their order under the same job, within about a minute. A server that starts while another still
runs a job leaves that job alone, and a job whose server stopped without a sign of life for a minute
is continued the same way. Summaries and walkthroughs already written are kept, so a continued run
does not pay for them again. It keeps clones of repositories under `/data`; a volume there saves cloning
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

Every response carries an `X-Request-Id`. Send your own (up to 64 letters, digits, `-`, `_` or `.`) and
it is kept; otherwise the server makes one. The id is on every log line written while the request is
handled, on its access line and its trace span, and an indexing job records the id of the request that
started it. Each request gets one access line at info level: method, path without the query string,
status, time in milliseconds, the request id and the member's email when signed in; never tokens or
cookies.

## Trying it locally

```sh
scripts/docker-local.sh      # builds the Linux engine here, packages comindly:local, makes .secrets/
docker compose up
```

Open <http://localhost:7878>, sign in as `admin@example.com` and read the link at
<http://localhost:8025> (Mailpit). `COMINDLY_SIGN_IN=code docker compose up -d` switches to codes;
the admin token for the first sign-in is in `.secrets/admin_token`. The index goes to a local S3-compatible store (SeaweedFS).
`docker compose down -v` removes everything.
