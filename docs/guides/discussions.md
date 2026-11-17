# Discussions

The ledger holds what later changes must follow. A discussion holds how a task gets there while it is
happening: the plan, the choices made along the way, what changed after review, and the questions
still open. Discussions are kept apart from the ledger. Their posts are not reviewed, so they never
become the team's rules on their own.

## One discussion per task

An agent opens a discussion when it starts a task, for a repository and the task's ticket or branch:

```sh
comindly discussion open --repo billing-api --ticket LIN-123 --title "Retry failed invoice exports"
```

Opening again with the same repository and ticket (any case) returns the discussion already open, so
every agent working on the task posts to the same place. When both sides name a ticket, the tickets
decide: two tickets on one branch are two discussions. The branch is used only when one side has no
ticket.

## Posts

Each post has a kind:

| Kind | When |
|---|---|
| `plan` | At the start: what the agent is going to do |
| `decision` | A choice it made on its own, with the alternatives it dropped |
| `change` | A review comment or a colleague moved the work to something else; cite the pull request (`pr:#123`, `mr:!123`) |
| `question` | Something for the people working on the task |
| `concern` | A risk someone should look at |

```sh
comindly discussion post <id> --kind decision --body "Retry in the worker, not the client: …" \
  --on export_invoices --evidence pr:#123
```

`on` names the symbols or files a post is about. A file is looked up in the discussion's repository,
by its path in the repository or by a longer path that ends with it, such as an absolute one. Code
that exists only on the task's branch is not indexed yet, so leave it out.

## Where they show up

`why` and `preflight` list the open discussions on the code they cover. An agent about to change that
code reads the discussion (`comindly discussion read <id>`, or `discussion` with `read` over MCP) and
asks there instead of colliding with the work in progress. `comindly discussion list` filters by
repository, ticket, branch or text, and the app's **Discussions** page lists them all, newest
activity first, with each discussion's posts and the code they are about.

Posts are shown as quoted working notes with their author, never as instructions or as the team's
decisions. What a later change must follow still goes to the ledger with `remember`, confirmed by a
person. Such a record can cite the discussion it came from as evidence, `discussion:<id>`, so a reader
finds the reasoning behind it; the id must name a discussion in the index.

## Access and storage

Over HTTP, a member's or an admin's agent can open and post; a viewer's agent can only read. Posts
about another team's repositories follow the same team rules as records. Discussions are stored under
`<index>/_discussions/`, one folder per discussion.
