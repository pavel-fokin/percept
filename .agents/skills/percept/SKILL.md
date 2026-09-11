---
name: percept
description: Select, check, and revise percept's cognitive maps; query its experience log. Use for decision rationale, corrections, dependencies, and map maintenance.
---

# Using shared maps

Read [.percept/index.md](../../../.percept/index.md) to choose a map.
`percept maps list` prints each map's purpose and size. The installed
binary is `~/.percept/bin/percept` and reads `~/.percept/percept.jsonl`.
A worktree build, `target/debug/percept`, reads `<checkout>/.percept/`
instead unless `PERCEPT_HOME` says otherwise, so set it to
`$HOME/.percept` to query the shared log with a dev build.

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
md` prints the rendered Markdown instead, cut to the fragment. A map
is folded live from the log on every read; no file holds a render.
`percept maps list --format md` prints one section per map: its
purpose and size, its node and edge kinds each with a line on what it
is, and one example node and edge.

The model's `read_map` tool takes the same `around`, `depth`, and
`kinds`. It returns a line naming the map's kinds and their meanings,
then the count line - what was left out, and how many edges cross the
cut - then the nodes and edges. Read the kinds line before picking an
`around` selector. Widen the cut when an exception, a contradiction, or
a consequence could change the answer.

Percept follows edges; it does not rank or decide. Without a relevant
node, search the log instead of inventing one.

## Check a claim

Every node and edge names the events it was drawn from and who wrote it:
`human`, `agent`, or `system`. The render marks agent-written nodes with
`(agent)`. Read the cited events before correcting a decision, resolving
a contradiction, or resting a consequential inference on an agent node:

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
`--actor agent`. Write several nodes at once with `percept maps
record`, one with `add-node`, or mid-turn with `revise_map`.
Capture an unlogged prompt with `events publish` under its real actor
and source before citing it; never invent an id or cite an agent's
summary as the user's words.

## Cite a file

A node that rests on code or a document cites the text it rested on,
not the path alone. In a `maps record` document that is one line under
the node, `cites src/mapstore/schema.rs:40-58` - a range where one is
enough, the whole file only for a claim about the file itself. For a
single node, publish what was seen first and list the printed id in
`--source` beside the prompt:

```sh
id=$(percept events publish --actor agent --source claude-code --type file.cited \
  --payload '{"path":"src/mapstore/schema.rs","lines":"40-58"}')
percept maps add-node decisions --actor agent --kind decision --name "..." \
  --prop why="..." --source $prompt --source $id
```

`file.cited` is experience: the file, or that range of it, as it was
seen at that moment. Publish reads the text from the tree, so the path
may be absolute inside the checkout or repo-relative, and refuses a
binary file or a range past the end. That text is what the
session-start block checks against the tree later: `changed` when it
is no longer found, `gone` when the file is. After looking at a
changed file, publish a new `file.cited` with `--causation $old`
whether or not the claim still holds; the check follows that chain and
reads the newest. If the meaning moved, record the new decision with a
`supersedes` edge as usual, citing the new event.

## Record a task

The tasks map holds work left to do. A `task` says in its `why`
property what it costs to leave undone; the store refuses one without
it. Its `state` property is `open`, `done`, or `dropped` - `open`
until something changes it. A task `blocks` the one that must wait for
it. Rewording a task changes its name in place, never a new node. The
model may reword only a task it wrote; on a task the user wrote it may
set `state` and nothing else, and once the user has changed a task the
model wrote, the same lock holds there.

```sh
percept maps add-node tasks --actor agent --kind task --name "cancel a turn without quitting" \
  --prop why="Esc drops the whole session on a fifty-call turn" --prop state=open --source $id
```

Close a task, drop it, reopen it, or reword it by changing the node it
already is - `percept maps record`'s document grammar takes a bare
short id, `t4`, for that:

```sh
percept maps record tasks --actor agent --source $id <<'EOF'
t4
  state "done"
  why "1f1a9a9: cancel a turn without quitting"
EOF
```

Under an existing node, `why` is the change's why - what happened -
not the task's own `why` property. A dropped task carries `state
"dropped"` and a why saying why; a reopened one `state "open"`.

Open on `percept maps show tasks --format md` before planning, so the
next item is picked rather than re-derived; close it at commit, with
the commit in the change's why.

## Add a map

A map is declared by a TOML file at `.percept/schemas/<name>.toml`;
the next `percept maps` command folds it. Kinds are lowercase, each
with a gloss a reader meets in `maps list --format md`, and a node
kind may list the properties a node must carry, and `state = [...]`
the values its `state` may hold - a set, no value open by position; a
node of that kind names one when added. `headlines` names the kinds
the prompt carries.

```toml
name = "glossary"
purpose = "what a term means in this project, so a word is not redefined"
headlines = ["term"]

[[node]]
name = "term"
gloss = "a word and the meaning this project gives it, in its `meaning` property"
requires = ["meaning"]

[[edge]]
name = "relates"
gloss = "from a term to one it is defined against"
from = "term"
to = "term"
```

`decisions` and `tasks` are built in as the same TOML; a project file
of the same name extends one - keep every kind and headline, add
more - and is refused if it drops any, since the log and
the render rest on them. `index` cannot be declared: it is this
directory's index. Add a row to `.percept/index.md` so a reader finds
the new map.

## Revise when meaning changes

Revise when evidence changes what a node means, an exception appears, a
constraint moves, or the user says the map misread them. One more event,
or a shorter wording, is not a trigger. "Nothing to revise" is a valid
result.

A correction is an addition. A decision you doubt is raised, not
replaced: a `question` with a `reopens <id>` line under it puts the
decision in the user's review, and the decision stands until they
settle it. A new decision the user agreed supersedes the old one:

```sh
percept maps add-edge decisions --kind supersedes \
  --from 'decision:<new>' --to 'decision:<old>' --source $id
```

The old node stays, one hop away, the `supersedes` edge printed under
both. A user-written node is the user's landmark: the model may attach
edges to it and set its `state`, and the map refuses anything else - a
rename, another property, a removal, or removing a user-written edge.
The same lock falls on a model node once the user has changed it: their
why is the node's last change, printed wherever the node is, and the
model's answer is a new node beside it, citing the prompt. Removing any node drops its edges with it; look before you do.
Nothing already rendered moves when a node is added: questions keep
their first-seen order and their raising prompt as the heading.

Report what changed in meaning, the evidence for it, and which decisions
it touches. A map is neither party's internal understanding; it is
where the two are compared.
