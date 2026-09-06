---
name: percept
description: Select, verify, and revise percept's cognitive maps; query its experience log or code structure. Use for decision rationale, corrections, dependencies, and map maintenance.
---

# Using shared maps

Read [.percept/index.md](../../../.percept/index.md) to choose a map.
`percept maps list` lists live maps, purposes, and sizes. The installed
binary is `~/.percept/bin/percept`; a worktree build is `target/debug/percept`.
For code query patterns, read [code.md](code.md).

## Select a fragment

Name the question the fragment must answer. Start with a commitment
from `percept maps show decisions --kind commitment`, then expand:

```sh
percept maps show decisions --around 'commitment:Event storage' --depth 2
```

The model's `read_map` tool accepts the same neighborhood through
`around: {kind, name}` and `depth`. It returns source IDs and counts of
omitted nodes and edges. A count is a boundary notice, not assurance
that omitted material is irrelevant. Expand a boundary when an exception,
contradiction, or consequence could change the answer.

Select by relevance yourself. Percept traverses explicit relationships;
it does not rank evidence or decide which conclusion to believe.
Without a relevant map entry, search the log instead of inventing one.

## Verify a claim

Use maps for orientation. Read cited events before changing a commitment,
resolving a contradiction, or relying on an unsupported consequential inference.

```sh
percept events show EVENT_ID
percept events search --contains 'search phrase' --size 10
```

Inspect the proposal behind a bare “approved” message. A valid citation
proves the event exists; it does not prove the interpretation follows.
When correcting a claim, follow its relationships and name the dependent
conclusions that need review. Relationships may be incomplete; inspect
the relevant code and search history as the task warrants.

## Record a lasting commitment

Apply AGENTS.md's admission rule. Keep routine configuration and progress
in the log. A commitment combines a lasting choice, rationale, scope,
and meaningful exceptions. Its short name is a familiar reference.

Use `commitment` nodes for the overview. Link supporting decisions,
questions, options, or evidence with `details` edges from the commitment.
This groups material without deleting or replacing its identities.
Add a relationship only when its meaning is supported; proximity in a
conversation does not prove a dependency or agreement.

Use existing string properties for `why`, `scope`, and `status` where
they help. Status is `proposed`, `agreed`, or `disputed`; missing status
means recorded, with agreement unknown. Mark `agreed` only with a cited
human statement supporting the actual commitment. Approval to build
does not ratify every interpretation the agent later records.

Write through `percept maps` or `revise_map`, citing source event IDs.
Never hand-edit the generated decisions Markdown. Capture an unlogged
prompt through `events publish` with its actual actor and source before
citing it. Do not invent an event ID or pretend an agent summary is a
verbatim user statement.

## Revise when meaning changes

Reconsider a map when evidence changes a claim, an exception appears,
a constraint changes, or human feedback exposes a misunderstanding.
Another event or a shorter possible wording is not itself a trigger.

Prefer a cited evidence node and relationship for a local correction.
The current mutation API cannot edit a node in place. Removing a node
also removes its relationships; inspect these before any replacement.
Keep disputed claims visible until their disagreement is resolved.
Discuss changes to familiar names and grouping before reorganizing them.

Report the semantic change, its evidence, and affected conclusions.
“No revision needed” is a valid result. Shared maps expose interpretations
for discussion; they are not either participant's internal understanding.
