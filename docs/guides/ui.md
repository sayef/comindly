# Desktop app, web and terminal UI

## Desktop app (macOS)

`comindly app [--index-dir …]` opens the UI in its own window, like the Ollama app: the operating
system's WebKit, no browser and no bundled Chromium. The window shows a loading screen while the
index loads, then the UI from a private server on a random local port that only this window can
call. Links to other sites open in your browser.

Started from a terminal, it sees your shell's environment (storage credentials for an index in a bucket). On
Linux and Windows, `comindly app` opens the same app in your browser instead: the local web UI, or the
server's address when you use one.

Against a server (after `comindly login <url>`, or with `--server <url>`), the window opens that
server's app instead, and the title shows which server. You sign in in the window as in a browser,
and it remembers the session. A sign-in link from an email opens in your browser; to sign the window
in with it, copy the link and choose **View → Open Link from Clipboard** (<kbd>⌘L</kbd>). `--local`,
or an explicit `--index-dir`, opens this machine's index as before.

## Web UI

`comindly ui --web [--port 7878] [--open]` serves the same UI on 127.0.0.1 only. A team server runs
with `comindly server` instead ([Self-hosting](self-hosting.md)). It prints a link
with an access token; open it once (or pass `--open`) and the browser keeps the token in a cookie,
so the plain address works as a bookmark afterwards. Other programs and web pages cannot call the
UI's API without it. The token is kept in `~/.config/comindly/ui-token`; delete the file to issue a
new one.

- **Overview**: counts, the org map, a version slider to compare with earlier index versions, what needs
  attention, the flows that reach furthest, symbols per repository and version skew. The org map is a
  tree from the entry points (or any repository you pick) to what each uses, or turned around to what
  uses it; click a box to open it, and each branch names the contract it goes through. A repository
  already shown elsewhere is dashed, and where repositories depend on each other in a circle an amber
  line loops back; **↺ cycles** lists them all.
- **Contracts**: every route, queue, topic, bucket and table with its providers and consumers, filtered
  by type, by whether both sides are indexed, and by name.
- **Ledger**: records that need review (challenged, stale or orphaned, and unconfirmed machine
  records) and the full timeline. **Add record** writes a new one under your name; each record shows
  who wrote it and its review history; confirm, archive, invalidate or delete it. The symbol view
  has **Add to the ledger** with that symbol filled in.
- **Flows**: where work starts and where it travels. Each flow follows calls and then crosses
  contracts: an HTTP route to the handler that serves it in another repository, a topic to the queues
  subscribed to it, a queue to the code that polls it. The list shows the repositories each flow
  passes through in order and the contracts it crosses; a flow opens as a tree of every step, with the
  steps that enter another repository marked. With LLM summaries on when indexing, the furthest-reaching
  flows also get a written walkthrough.
- **Conventions**: every measured and team convention in one table, filtered by scope (org-wide or a
  repository), topic, origin and text, and sorted by scope, topic or how widely it is followed. Expand a
  row to see where it comes from: the places that follow it and those that deviate, each with its code.
  Add your own, edit or hide measured ones (hidden ones can be restored). `/conventions?repo=<name>`
  opens one repository.
- **Packages**: internal packages, who publishes each, and the version every consumer pins, marked
  when it is behind the publisher (or ahead of the version in its source).
- **Settings → Repositories**: what is in the index, and where each repository lives on this machine.
  **From GitHub or GitLab** lists the repositories of an organisation or group (github.com, GitHub
  Enterprise, gitlab.com or a self-hosted GitLab, subgroups included) with your existing git login, a
  `GITHUB_TOKEN` / `GITLAB_TOKEN`, or a token it asks for once (kept by your git credential helper). Tick
  the ones you want: comindly keeps its own copies under `~/.local/share/comindly/repos` (cloned on the
  first **Index now**, without file contents until needed), so you choose no folder. With **Share with the
  team**, the list is stored in the team settings, and a colleague sees "The team has N repositories you
  don't have here" with **Add them**. You can also add a folder, scan a folder, or clone a single
  repository by URL. Every repository is always indexed at its remote's current default branch, whatever
  it is called (main, master, trunk…), asked from the remote on each run. The index records each
  repository's remote URL, so one indexed elsewhere shows **No folder here** with **Locate** and **Add**.
  comindly indexes all repositories together so cross-repo links stay complete, so **Index now** waits
  until every indexed repository has a folder here, unless you choose to drop the missing ones. With
  **Re-index automatically** on (`auto_index`), the app checks every freshness interval whether a default
  branch moved on the remote and re-indexes without LLM steps when one did; it is one of the index's
  shared settings.
- **Settings**: the index location (with a connection test), the index's shared settings (LLM
  endpoint and model, embedding model, LLM steps, limits and prices), your own API key, your name on
  ledger records, and how often to check for a new index version. Changing the index location switches
  the UI without a restart. On a team server, Settings also has **Members** and **Integrations**
  (admins) and **Your access** (your sign-in code, personal access tokens and how to connect an agent);
  viewers see everything read-only.
- **Activity**: what comindly works on in the background. Tiles count what runs, waits, succeeded and
  failed (click one to filter); the list groups runs under **Now**, **Today**, **Yesterday** and earlier
  days. A run opens as a timeline: one row per step with its duration, its results, notes, warnings and
  errors, a live progress bar for the step in progress, and the LLM tokens and cost. **Plain log** shows
  the text to copy into an issue; the request id finds the run in the server's logs. Admins can cancel a
  run or remove it from the queue. The server keeps the last 50 runs.
- **Approving a command line**: `comindly login` opens `/device?code=…`; the page shows which machine
  asks to act as you and the code its terminal shows, with **Approve** and **Deny**.
- **Get started**: checks of this setup (as `comindly doctor`), how to register the MCP server with
  Claude Code, Cursor, Codex or VS Code with this index's path filled in, hooks, and every MCP tool
  with a question that uses it.
- **Symbol view**, opened from search, a link or a graph node: the dependency graph (dependents
  on the left, dependencies on the right), blast radius, the LLM summary and flow walkthrough when
  the index has them, contracts and recorded reasoning. **Copy context for an agent** copies what to
  read before changing it, with the team's records, as markdown.

### What it looks like

The example company in `examples/acme`, indexed with comindly (`scripts/screenshots.sh`).

![Overview: the org map with the service contracts between repositories](../assets/screenshots/overview.webp#only-light)
![Overview: the org map with the service contracts between repositories](../assets/screenshots/overview-dark.webp#only-dark)
/// caption
Overview: the org map with the service contracts between repositories
///

![A symbol: its dependency graph, blast radius, summary and flow walkthrough](../assets/screenshots/explorer.webp#only-light)
![A symbol: its dependency graph, blast radius, summary and flow walkthrough](../assets/screenshots/explorer-dark.webp#only-dark)
/// caption
A symbol: its dependency graph, blast radius, summary and flow walkthrough
///

![Contracts: routes, topics, queues, buckets and tables with providers and consumers](../assets/screenshots/contracts.webp#only-light)
![Contracts: routes, topics, queues, buckets and tables with providers and consumers](../assets/screenshots/contracts-dark.webp#only-dark)
/// caption
Contracts: routes, topics, queues, buckets and tables with providers and consumers
///

![Ledger: constraints, failed attempts and decisions, with who wrote and confirmed them](../assets/screenshots/ledger.webp#only-light)
![Ledger: constraints, failed attempts and decisions, with who wrote and confirmed them](../assets/screenshots/ledger-dark.webp#only-dark)
/// caption
Ledger: constraints, failed attempts and decisions, with who wrote and confirmed them
///

![Packages: an internal package and a consumer pinned behind its publisher](../assets/screenshots/packages.webp#only-light)
![Packages: an internal package and a consumer pinned behind its publisher](../assets/screenshots/packages-dark.webp#only-dark)
/// caption
Packages: an internal package and a consumer pinned behind its publisher
///

![Flows: where work travels, across repositories](../assets/screenshots/flows.webp#only-light)
![Flows: where work travels, across repositories](../assets/screenshots/flows-dark.webp#only-dark)
/// caption
Flows: where work travels, across repositories
///

![A flow: every step, entering orders, inventory and payments](../assets/screenshots/flow.webp#only-light)
![A flow: every step, entering orders, inventory and payments](../assets/screenshots/flow-dark.webp#only-dark)
/// caption
A flow: every step, entering orders, inventory and payments
///

![Conventions: one table, with where each comes from](../assets/screenshots/conventions-open.webp#only-light)
![Conventions: one table, with where each comes from](../assets/screenshots/conventions-open-dark.webp#only-dark)
/// caption
Conventions: one table, with where each comes from
///

![Search: describe what you are looking for](../assets/screenshots/palette.webp#only-light)
![Search: describe what you are looking for](../assets/screenshots/palette-dark.webp#only-dark)
/// caption
Search: describe what you are looking for
///

![Activity: indexing runs grouped by day, one opened as a timeline of its steps](../assets/screenshots/activity.webp#only-light)
![Activity: indexing runs grouped by day, one opened as a timeline of its steps](../assets/screenshots/activity-dark.webp#only-dark)
/// caption
Activity: indexing runs grouped by day, one opened as a timeline of its steps
///

![Get started: register the MCP server with your agent](../assets/screenshots/setup.webp#only-light)
![Get started: register the MCP server with your agent](../assets/screenshots/setup-dark.webp#only-dark)
/// caption
Get started: register the MCP server with your agent
///

What is running shows in the header's corner (indexing with its current step, or any action in
progress), with **Cancel** where it can be stopped; indexing stops at its next checkpoint and keeps
what it already saved. Clicking it opens the run under **Activity**, whose sidebar entry counts what
runs and waits. Finished actions appear as toasts in the bottom-right corner.

Press <kbd>⌘K</kbd> (<kbd>Ctrl K</kbd>) anywhere to search: by name, and by describing what you are
looking for ("where do we retry HTTP calls"), which uses the same hybrid search as the MCP `search`
tool. Graphs scroll with the page; pinch or <kbd>⌘</kbd>/<kbd>Ctrl</kbd> and scroll to zoom, drag to
pan. Dark by default; the header toggles a light theme. Against a read-only index, editing is hidden.

## Terminal UI

`comindly ui` is the compact terminal view: overview, contracts, ledger (with a timeline), explorer and
conventions. On the ledger tab, <kbd>c</kbd> confirms and <kbd>x</kbd> retracts the selected record;
<kbd>r</kbd> reads the index again.

Against a server (after `comindly login <url>`, or with `--server <url>`), it shows the server's index
and names the server in its title. Reviews go to the server under your name; signed in as a viewer,
the help line says read-only and <kbd>c</kbd>/<kbd>x</kbd> explain why instead of sending a review. The
server's index changes as it re-indexes and colleagues add records: press <kbd>r</kbd> to fetch it
again. The whole view is fetched at once, compressed (about 10 KB for the example company; a large
index is tens of MB), and kept in your cache folder (readable only by you, removed by `comindly
logout`): starting again or pressing <kbd>r</kbd> downloads it only when it changed. `--local`, or an
explicit `--index-dir`, uses the index on this machine.
