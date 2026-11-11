# Concepts

**Symbols and identity.** Every function, method, class, file and contract has a global id in the SCIP
scheme (`comindly-treesitter . <repo> . <path>/<name>().`), so a definition in one repository and its users
in another meet on the same id. When a repository has a SCIP index, its compiler-grade answers are used
as well.

**Edges.** `imports`, `calls`, `instantiates`, `inherits` and `implements` between symbols, within and
across repositories. The resolver binds names through imports, re-exports, type annotations and return
types; when it cannot bind a name it leaves the edge out rather than guess.

**Contracts.** A route, queue, topic, bucket or table is a node of its own. Code that serves, publishes or
writes it is a provider; code that calls, subscribes or reads it is a consumer. Values come from literals,
constants, settings defaults, environment variables, YAML config and Terraform; deployment tiers
(`dev`, `stage`, `prod`) are not part of a resource's identity.

**Blast radius.** `ripple` walks reverse dependencies, through contracts, across repositories: changing a
route reaches every client of it.

**Versions.** Each build writes a new version of the index; readers always see a complete one. The web UI
can show the org map at any earlier version.

**The ledger.** Typed records (`attempt`, `decision`, `constraint`, `gotcha`, `intent`, `question`)
anchored to symbols with a fingerprint of the code they describe:

| Status | Meaning |
|---|---|
| active | standing; notes say where its code moved, was renamed or changed since, and the commit it saw |
| orphaned | all of its code is gone |
| superseded / retracted | retired by a person or an agent acting for one; archive and invalidate retract a record |

| Trust | Source |
|---|---|
| T4 | a constraint whose check passes on the current code |
| T3 | written or confirmed by a person |
| T2 | an agent, with evidence |
| T1 | an agent without evidence |
| T0 | untrusted (failed signature, unsafe content) |

Hooks and context packs only use T2 and above.
