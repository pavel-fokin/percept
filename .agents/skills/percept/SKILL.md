---
name: percept
description: Select, check, and revise percept's cognitive maps; query its experience log or code structure. Use for decision rationale, corrections, dependencies, and map maintenance.
---

# Using shared maps

Read [.percept/index.md](../../../.percept/index.md) to choose a map.
`percept maps list` prints each map's purpose and size. The installed
binary is `~/.percept/bin/percept` and reads `~/.percept/percept.jsonl`.
A worktree build, `target/debug/percept`, reads `<checkout>/.percept/`
instead unless `PERCEPT_HOME` says otherwise, so set it to
`$HOME/.percept` to query the shared log with a dev build. For code
query patterns, read [code.md](code.md).

## Select a fragment

Name the question the fragment must answer, then cut the map to it:

```sh
percept maps show decisions --around 'question:Where does the event log live?'
percept maps show decisions --around 'question:Where does the event log live?' --depth 2 --kind evidence
percept maps show decisions --since 1d
```

`--around` keeps one node and what is within `--depth` edges of it, in
either direction; `--kind` then keeps only those kinds. `--since` keeps
what was added from that instant, and cuts after `--around`, so the two
together read as "what changed near this node". A filtered read prints
one line on stderr: how many nodes and edges were shown of the total,
and how many edges cross the cut. stdout is JSONL by default; `--format
md` prints the rendered Markdown instead - the same text as
`.percept/<map>.md`, cut to the fragment. `percept maps list --format
md` prints the catalogue as a table.

The model's `read_map` tool takes the same `around`, `depth`, and
`kinds`, and returns the same counts as its first line. A count says
something was left out, not that it did not matter. Widen the cut when
an exception, a contradiction, or a consequence could change the answer.

Percept follows edges; it does not rank or decide. Without a relevant
node, search the log instead of inventing one.

## Check a claim

Every node and edge names the events it was drawn from and who wrote it:
`user`, `model`, or `system`. The render marks model-written nodes with
`(model)`. Read the cited events before correcting a decision, resolving
a contradiction, or resting a consequential inference on a model node:

```sh
percept events show EVENT_ID
percept events search --contains 'search phrase' --size 10
```

A citation proves the event exists, not that the reading follows from
it. A bare "approved" cites the proposal it approved; open that. When
correcting a claim, follow its edges and name the decisions that rest
on it.

## Record a decision

The recipe is in the [plan skill](../plan/SKILL.md): a `question`, the
`decision` with a `resolves` edge, and an `option` with an `answers`
edge for each alternative that lost, saying why in its `why` property.
Every node cites the prompt that settled it and is written as
`--actor model`. Write through `percept maps` or `revise_map`, never
by editing the rendered Markdown.
Capture an unlogged prompt with `events publish` under its real actor
and source before citing it; never invent an id or cite an agent's
summary as the user's words.

## Record a task

The tasks map holds work left to do. A `task` names one outcome and
says in its `why` property what it costs to leave undone; the store
refuses one without it. An `outcome` with a `resolves` edge settles it,
done or dropped, with the commit or the reason in its `ref` or `why`
property. A task `blocks` the one that must wait for it. Rewording a
task is a new task with a `supersedes` edge, never a removal.

```sh
percept maps add-node tasks --actor model --kind task --name "cancel a turn without quitting" \
  --prop why="Esc drops the whole session on a fifty-call turn" --source $id
percept maps add-node tasks --actor model --kind outcome --name "done in 1f1a9a9" \
  --prop ref=1f1a9a9 --source $id
percept maps add-edge tasks --actor model --kind resolves \
  --from 'outcome:done in 1f1a9a9' --to 'task:cancel a turn without quitting' --source $id
```

Open on `.percept/tasks.md` before planning, so the next item is picked
rather than re-derived; add the outcome at commit.

## Revise when meaning changes

Revise when evidence changes what a node means, an exception appears, a
constraint moves, or the user says the map misread them. One more event,
or a shorter wording, is not a trigger. "Nothing to revise" is a valid
result.

A correction is an addition. A new decision supersedes the old one:

```sh
percept maps add-edge decisions --kind supersedes \
  --from 'decision:<new>' --to 'decision:<old>' --source $id
```

The old node leaves the headlines and renders as `was` under its
successor, one hop away. A user-written node is the user's landmark:
the model may attach edges to it, but `revise_map` refuses to remove it,
to remove a user-written edge, or to remove a model node a user edge
touches. Removing any node drops its edges with it; look before you do.
Nothing already rendered moves when a node is added: questions keep
their first-seen order and their raising prompt as the heading.

Report what changed in meaning, the evidence for it, and which decisions
it touches. A map is neither party's internal understanding; it is
where the two are compared.
