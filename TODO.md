# To do

Open items: things to try, measure or set up. Remove an item once it is done.

## To try

- **Team pilot** on two or three repositories: shared index on S3 from CI, hooks and MCP for a few
  developers, preflight in their MR pipelines.

## Dependencies

- **lancedb 0.37.1 pin.** 0.38–0.40 need the heavy `remote` feature. When a later version builds without it,
  move up, drop the pin from Dependabot, and remove the quick-xml advisory ignores from `deny.toml`.

## Gaps

- **Large graphs in the web UI.** Canvas rendering is fine for the org map and per-symbol views; a whole-org
  symbol graph would need WebGL.
- **Helm values** as a source of queue, topic and bucket names (no repository to test against yet).
