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

The team picker in the header filters the org map, flows, contracts, explorer, ledger, packages,
conventions and repositories to the teams you pick: one, several, **Unassigned**, or **Everyone**. The
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

## Agents

The MCP tools keep answering across the whole org. Labels let you steer them: a review bot's
instructions for one repository can say which team to prefer.
