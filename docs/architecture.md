# The core and its surfaces

A proposal, written 2026-09-08. It names what percept's core is, what
sits outside it, and how the outside reaches it. `docs/harness.md`
designs one surface, the coding agent; this document is the level
above it. Nothing here changes behaviour on its own.

## The claim

percept's value is not memory. Every coding client now keeps one.
The core adds three things a memory lacks:

| Property | What it means | Where it lives today |
|---|---|---|
| Evidence | A claim in a map cites the experience it came from. A reader can check it. | `source` on every node and edge event. |
| Co-ownership | A map is shared by the human and the agent under one rank rule: a node's name and its removal belong to its writer and to anyone above, and a change from above locks the node below. The human's correction sits on the node as its last change. | `Map::apply`, W6; `actor`, `changed_by`, `changed_why` on a node. |
| One log | The log spans clients and projects. What one agent learned another can fold. | `~/.percept/percept.jsonl`, `Source` on every event. |

The core is what carries those three. Anything that does not is a
surface.

## The rules

The core is cognitive rails: constraints on what cognition may write
and read. It is not a cognitive architecture in the SOAR or ACT-R
sense, which prescribes how thinking proceeds, and not a guardrail on
what the model says. The loop is a surface's, and any model or client
may run it. The core is the rules, and everything else is built around
them.

| Rule | Enforced by |
|---|---|
| Experience is append-only and never changes. | The log store. |
| A claim in a map cites the experience it came from. | `source` required on every map event. |
| A map is folded deterministically from cognitive commits, and the fold has one implementation. | `Map::apply` in Rust. Nothing stops a second fold yet. |
| An actor renames or removes only its own node or one below its rank; a change from above is not undone from below. | `Map::apply`, W6. |
| An alternative is recorded only with the reason it lost. | `requires` on the kind in its schema; the shared write path refuses a node missing one. |
| A recorded claim is not the human's agreement; silence stays claimed. | Prose. The core keeps no standing: `changed_by` says who last touched a node, and nothing says who agreed. |
| percept never ranks, summarises, or answers; output is constant-size per event. | Prose, and the habit of the search tools. |
| One log, one writer. | `App::commit` for the loop; the CLI writes on its own. |

A rule enforced only in prose is a value, not a constraint. Each such
row is either a task, to give it a refusal in code, or an honest label
as a value the surfaces are asked to keep. Both are fine; mixing them
is what erodes a rule set.

A rule enters the core only when a surface cannot be trusted to keep
it, and it pays its way against one of the three budgets or one of the
three claims. The window, the tool policy, the render, and the `code`
map fail that test and sit outside. Schemas as data and the lifecycle
are mechanism rather than rule, and are in the core only because the
rules need something to attach to.

## The core

Small: events, maps, and the rules between them. This section is the
specification, written 2026-09-11: what the core holds, what may be
done to it, and what must stay true. `src/core` implements it and
nothing more.

### Types

```
Actor     = Human(id?) | Agent | System
rank      : Human → 2, Agent → 1, System → 1
outranks(a, b)  = rank(a) > rank(b)
owns(a, x)      = a == x.actor            (Agent == Agent: an agent carries no id yet)
may(a, x)       = (owns(a, x) ∨ outranks(a, x.actor)) ∧ ¬outranks(x.changed_by, a)

Schema    = { name, purpose, headlines: {kind}, node_kinds, edge_kinds }
NodeKind  = { name, gloss, prefix, requires: {key}, states: {value} }
EdgeKind  = { name, gloss, from: {kind}, to: {kind} }

Node      = { id, seq, kind, name, properties: key → value, sources: [EventId],
              actor, added_at, changed_at, changed_by, changed_why? }
Edge      = { kind, from: NodeId, to: NodeId, sources, actor, added_at }
Map       = { schema, nodes, edges }
```

`states` is a set: no value is the open one by position. `changed_by`
and `changed_why` are the node's last change, who and why. They are
what a correction looks like on the node it corrects.

Invariants, true of every map:

| | |
|---|---|
| I1 | `(kind, name)` is unique among live nodes. |
| I2 | `(kind, from, to)` is unique among live edges. |
| I3 | Every edge's `from` and `to` are live nodes. |
| I4 | `seq` is minted per kind, monotonically, and never reused. |

### Events

Every event carries `id`, `actor`, `source`, `causation_id?`, and
`created_at`. Five payloads change a map:

```
node.added    { map, node, seq, kind, name, properties, sources }
node.changed  { map, node, name?, properties, sources, why? }
node.removed  { map, node, why, sources }
edge.added    { map, kind, from, to, sources }
edge.removed  { map, kind, from, to, why, sources }
```

`why` is carried where something a reader has seen changes or goes. A
new node's reason is its own property, when `requires` asks for one.

### Write rules

`Map::apply(mutation, actor)` checks these before anything is
appended. A refusal is an error, and no event exists.

| | Rule | On |
|---|---|---|
| W1 | `kind` is in the schema. | `node.added`, `edge.added` |
| W2 | `name` is not blank; I1. | `node.added`, `node.changed` with a name |
| W3 | `requires ⊆ keys(properties)`. | `node.added` |
| W4 | When the kind declares states, `properties.state ∈ states`: required on add, checked when sent on change. | `node.added`, `node.changed` |
| W5 | `from.kind ∈ edge_kind.from`, `to.kind ∈ edge_kind.to`; I2. | `edge.added` |
| W6 | Rank. A rename, a property other than `state`, or a removal needs `may(actor, node)`; removing an edge needs `may(actor, edge)`. `state` and a new edge are any actor's. | `node.changed`, `node.removed`, `edge.removed` |

W6 is everything the core knows about who may do what, and no rule
names a kind. Because `may` weighs the node's last change, an agent
cannot rewrite or remove a node the human has touched. What it can
still do is add a node and an edge beside it, which is how a decision
is superseded, and the core never learns the word.

A change that carries only `why` is legal. It is a comment, and it
updates `changed_at`, `changed_by`, and `changed_why` like any change.

### Fold rules

`Map::replay(payload, actor, at)` applies what the log recorded. It
checks structure only, so a log written before a rule still folds.

| | |
|---|---|
| F1 | `node.added`: W1, I1; `seq = max(seq, next)`; insert with `actor`, `added_at = changed_at = at`, `changed_by = actor`. |
| F2 | `node.changed`: the node is live; apply `name`, merge `properties`, append `sources`; `changed_at = at`, `changed_by = actor`, `changed_why = why`. |
| F3 | `node.removed`: the node is live; drop it and every edge on it; `seq` is not freed. |
| F4 | `edge.added`: W1, I2, I3; insert with `actor`, `added_at = at`. |
| F5 | `edge.removed`: the edge is live; drop it. |

W3, W4, W5's ends, and W6 are never checked on fold.

### Queries

What the core reads out of a map, knowing no kind:

```
node(id) · find(kind, name) · resolve(short_id)
headlines()                     nodes whose kind is in schema.headlines
linked(id, edge_kind, dir)      the nodes across one edge kind, dir ∈ {from, to}
since(at)                       nodes with changed_at ≥ at, edges with added_at ≥ at
Selection { around, depth, since, kinds } → Fragment { nodes, edges, left_out, crossing }
```

### Not in the core

- Standing, read receipts, who has seen what. A since-cut runs from a
  time the caller gives: the hook from its source's last
  `session.started`, the review from its own, the shell from `--since`.
- Open and settled, a settling pair, an order on states. A question
  with no incoming `resolves` edge reads as open in the render, by eye.
- A successor chain. An edge is printed as an edge: `d9 supersedes d7`.
- A question to a node, or an obligation to answer one. That is a rule
  in the text a session starts with.
- Any kind or edge name: `decision`, `option`, `supersedes`, `reopens`,
  `resolves`, `answers`, `blocks`.

A schema is data: a TOML file at `.percept/schemas/<name>.toml`,
which a loader outside the core parses and hands in. `decisions` and
`tasks` ship as the same TOML, embedded, and a project file of the same
name extends one without shrinking it. The core folds any schema it is handed.

### Contours of two kinds, one surface

Several cognitions share a map: the agents that write to the log, and
the humans who work with them. Each is a contour with its own boundary,
and the boundaries come in two kinds. An agent's contour is inside the
log: what it searched, what it wrote, why. A subagent is a contour of
its own. A human's contour is mostly outside the log: their head, their
notes, what they said elsewhere. Only what they type crosses in. One
kind is transparent to the system and the other opaque, and the core
models a human no further than what crosses.

The surface is where contours meet: the nodes more than one has
touched. The core marks it with two things, and both are every map's,
the way every node carries an actor.

| Piece | What it is |
|---|---|
| `changed_by`, `changed_why` | The node's last change: who, and why. A human's "wrong" on an agent's node is a `node.changed` carrying a `why` and nothing else, and it stays on the node until the next change. |
| The rank lock, W6 | A change from above is not undone from below. After the human's why, the agent may add beside the node and never rewrite it. |

Silence stays claimed and nothing marks agreement: the core keeps no
standing and no read receipt. A model node the human read and left
alone looks like one they never opened, and the tally in
`docs/mvp.md` is what tells the two apart. A `confirms` and
`disputes` edge with a standing derived per confirmer was the design
until 2026-09-11; the decisions map holds why it lost.

Who a human contour is stays open. The core has an actor kind for the
human and no identity behind it. `changed_by` per person needs one
only when a second person writes, and the question waits for that person.

The maps do not split by contour. One map per contour multiplies what a
reader holds for a distinction the fold derives, so the contour is a
view over one map and never a storage boundary. The mixing today is
right; the surface is the nodes whose `changed_by` is not their `actor`.

This is what makes co-ownership an operation rather than a rule: a
correction with provenance between contours.

The domain is now two modules. `core` holds experience and maps; `harness`
holds the ports a loop needs to drive a model over them - `Model`, `Tool`,
`Snapshot`, and the policy that asks before a tool runs - and depends on
`core`. A Python SDK that folds a map never sees a `Model` trait.

One thing did not move. The core keeps the name `Policy` for the
ask-before-write gate, and the collaboration rule, W6, stays inside
`Map::apply` unnamed. Whether the core should reserve "policy" for the
collaboration rule is open.

The `code` map is a surface feature. It folds a working tree and not
the log, so it goes with the coding surfaces and reaches the core
through the `MapReader` port as it does now.

## The surfaces

Each depends on the core and never on another surface. Each brings
the practice of its own field.

| Surface | Reaches the core through | Brings from its field |
|---|---|---|
| Skills and hooks for a coding client | The `percept` CLI: `events`, `maps`. Instructions in `AGENTS.md`, skills under `.agents`, a hook per client. | The client's conventions: instruction files, skill formats, hook shapes. |
| Coding agent | The harness: context sections over the log, map tools, tool policy, snapshot. | Harness practice: prefix caching, subagents, permission modes, undo. |
| Web application | A server over the ports; the render as a page. | Sessions, sharing, review of a map by more than one person. |
| Python and TypeScript SDKs | The format, and the one fold reached through the CLI now and WASM or a C ABI later. | Idiomatic APIs. Thin: write events, call the fold, read fragments. |

The fold has one implementation, in Rust. An SDK that reimplements it
drifts. The JSONL line is the interface to version.

```
     skills · hooks      coding agent      web app      SDKs
           │                  │              │            │
           │ CLI              │ harness      │ server     │ format · fold
           ▼                  ▼              ▼            ▼
    ┌───────────────────────────────────────────────────────────┐
    │ core                                                      │
    │   Event · Map · Schema · rules · Selection · ports        │
    │   the JSONL format                                        │
    └───────────────────────────────────────────────────────────┘
                             │
                             ▼
                    ~/.percept/percept.jsonl
```

## Crates and targets

The `src/core` and `src/harness` modules now enforce the dependency
direction: `core` names no model, no tool, no tree. A workspace split is
mechanical from here. It waits for a second consumer: a crate boundary
with one consumer is cost with no check.

| Crate | Holds | Exists when |
|---|---|---|
| `percept-core` | Event, Map, Schema, fold, rules, Selection, the format. | The lib target is clean of harness ports. |
| `percept-harness` | Context, tools, tool policy, providers, snapshot, `code`. | The web app or an SDK needs the core without it. |
| `percept` binary | CLI over the core; the TUI under `--features lab`. | Now. |
| `percept-wasm` or FFI | The fold for SDKs and the browser. | The first SDK. |

The module split that tested the shape is done: the harness ports are
out of `core`, and it holds no model, no tool, and no tree. A library
target beside the binary is the next step that costs nothing.

Until then the split is a Cargo feature. `app`, `harness`, `code`,
`tui`, `providers`, and the tools the model calls build only under
`--features lab`, off by default, so the binary a developer installs
is `core`, `store`, `mapstore`, and the CLI, and a core change that
breaks the lab fails in the same tree.

## Validation

The two directions test different claims.

| Direction | Tests | Cost | Confound |
|---|---|---|---|
| Skill | Whether shared maps are worth their upkeep to a human and agent pair, with the strongest model doing the cognition. | Low: it runs on every session anyway. | The client's own memory does part of the job. |
| Coding agent | Whether the log as environment beats a transcript: the model searches what it cannot hold. | High. | A weak harness fails for reasons that are not the core's. |

A check for each:

- **Skill.** Over a run of sessions, count decisions reopened and plan
  steps re-derived, with the map in `AGENTS.md` and without it. Count
  how often the human corrects a node the model wrote, and what the
  correction cost. A core that helps shows fewer reopenings and cheap
  corrections.
- **Coding agent.** A task whose answer sits past the window. Watch
  whether the model reaches for `search_events` or `read_event` and
  lands it, against a raw transcript of the same length. `/context`
  says whether the prefix held.

A result that shows only "remembering helped" validates memory, not the
core. The three properties above are what the checks must show.

## Next steps

### Skill direction

1. Run the check on the sessions already happening. A tally per
   session: reopened, re-derived, corrected. No code.
2. Fix the two strains it has already shown: the render pulls unmerged
   branches' nodes, and the map has no budget. Both are open questions
   in the decisions map.
3. Schemas as data, so a session can add a map without a Rust change.
   Done: the first core addition the direction asked for.

### Coding agent direction

1. Confirm the harness works as designed: cached tokens rise across a
   tool round, and the window reaches a plan written before a long
   discussion. Both are open tasks.
2. Let instructions and maps give way under the budget on a small
   window, so the loop runs on any model.
3. Run the past-the-window check. Do not add subagents, plan mode, or
   permission modes before it passes: they are harness features, and
   the check is about the core.

### Both

1. The lib target. The harness ports are already out of `core`; a
   library beside the binary makes the shape checkable from outside.
2. The surface, built 2026-09-11 as the rank lock and the last change
   on a node. What remains is the tally that says whether it is read.

## Recommendation

Validate through the skill direction first. Value has already shown
there, the check costs nothing, and a negative result is about the
core and not the harness. Keep the coding agent as the lab for the one
claim only it can test, the log as environment. Do not judge the core
by how the coding agent codes until the harness is level with the
clients. If the skill direction comes back weak on evidence,
co-ownership, and one log, the core is a memory format with extra
ceremony, and that is worth knowing before a crate split or an SDK.
