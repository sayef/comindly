# Teams

One comindly index is one **org**: one graph, one ledger, one list of members. **Teams** are labels
inside it. They decide what you look at first, never what you may see: everyone in the org can still
open everything, and roles (admin, member, viewer) stay the same for the whole org.

## Labels

A team is a name with a list of repositories. A repository can carry several labels, for example a
shared library that two teams own. Everything in a repository carries its labels: symbols, files,
packages, conventions and ledger records. A flow carries the labels of every repository it crosses;
a contract those of its provider and its consumers. A repository without a label is **Unassigned**.

Admins set labels in **Settings → Repositories** (the Teams column, also when adding a repository) and
manage the team list in **Settings → General**, the Teams card: create, rename and delete teams, see
each team's repositories and members.

## Choosing what you see

The team picker in the header filters the org map, flows, contracts, ledger, packages, conventions
and repositories to the teams you pick: one, several, **Unassigned**, or **Everyone**. The
choice is part of the address (`?teams=payments,platform`), so a link opens the same view for a
colleague. Without one, comindly starts with your own teams.

On the org map, the repositories of teams you did not pick fold into one node per team at the edge,
with the links into your view, so dependencies across teams stay visible. Click one to add that team.

## Your teams

On a server with sign-in, everyone picks the teams they belong to in the team picker (**My teams**);
admins can also set them in the Teams card. Your teams decide your starting view.

## Upgrading

An existing server whose teams were never set up gets one team when it starts with this version: named
after the org, with every indexed repository, every member and every open invitation. It is saved as a
normal settings version by `comindly upgrade` and runs once; a deployment that later removes all teams
does not get it back. Rename the org afterwards if its name was really the team's, and add the next
team.

## Notes from other teams

Every ledger note remembers the teams its writer belonged to. A note about code that only other
teams own is marked **From <team>** and counts less until a member of an owning team confirms it:
an agent's note stays unverified, a person's stays below confirmed. The owning team chooses, in the
Teams card, how such notes count:

| Setting | Notes from outside |
|---|---|
| Count right away | Count like the team's own, still marked |
| Marked until we confirm (default) | Count less until a member confirms |
| Not accepted | Refused; the writer is asked to go through a member |

When a repository belongs to several teams, the most open setting wins. Notes written before teams
existed, and notes by people in no team, count as before. The ledger's **From other teams** view
lists the notes waiting for your teams.

## Agents

Agents keep seeing the whole org. The MCP tools `search`, `find`, `repos`, `contracts` and `suggest`
take an optional `team` argument (comma-separated names, `none` for unlabelled repositories);
`repos` lists the teams of each repository, and `ripple` counts the impact per team next to per
repository. A review bot's instructions for one repository can say which team to prefer.

In a terminal, `comindly search` and `comindly find` take `--team` the same way:

```sh
comindly find PaymentClient --team payments,platform
```
