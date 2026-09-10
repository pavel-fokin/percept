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
| Co-ownership | A map is shared by the human and the agent under landmark rules: a user-written node is never removed by the model, a decision is superseded and never deleted. The human confirms or disputes what the agent wrote. | `Map::apply`; the `actor` on a node. Confirmation is not built: see the surface below. |
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
| The model never removes what a user wrote; a decision is superseded, never deleted. | `Map::apply` refuses; `revise_map` says to supersede. |
| An alternative is recorded only with the reason it lost. | `requires` on the kind in its schema; the shared write path refuses a node missing one. |
| A recorded claim is not the human's agreement; silence stays claimed. | Prose only. The surface below is the enforcement, not built. |
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

Small: events, maps, and the rules between them.

| Part | Holds |
|---|---|
| Event | An append-only entry: id, actor, source, causation, time, payload. Never changes. |
| Map, Schema | Nodes and edges folded from `node.added`, `edge.added` and their removals. A schema names the kinds it allows and one line of purpose. |
| Rules | Who may remove what; an option needs a why; a decision is superseded, never removed. One place, `Map::apply`. |
| Selection, Fragment | A cut of a map around a node, since an instant, of some kinds, with counts of what the cut left out. |
| Ports | Append, load, search the log; read and render a map. |
| Format | The JSONL line. The contract every language speaks. |

A schema is data: a TOML file at `.percept/schemas/<name>.toml`,
which a loader outside the core parses and hands in. `decisions` and
`tasks` ship as the same TOML, embedded, and a project file of the same
name extends one without shrinking it. The core folds any schema it is handed.

Two things belong here that the code does not have yet.

- **Lifecycle on a schema.** A schema says which edge kinds move a node
  between which states: `resolves` makes a question settled,
  `supersedes` makes a decision past. The fold derives the state; the
  renderer stops branching on kind names.
- **The surface between contours.** Below.

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
touched. The rule "a recorded claim is not the human's agreement" names
it, but today it has no operation behind it. A user-written node is a
human's, a model node is an agent's, and a model node a human has read
and not objected to is in limbo.

The mechanism is one edge and one derived state, and it is the core's,
not any one map's. Every schema carries it, the way every schema
carries an actor on a node.

| Piece | What it is |
|---|---|
| `confirms`, `disputes` | An edge from a human contour to a node another contour wrote. An event like any other: it cites its prompt and never changes. |
| Standing | Derived by the fold, per confirmer: claimed, confirmed by whom, disputed by whom. A user-written node is confirmed by its writer by construction. |

Only a human contour confirms. An agent confirming another agent's node
is a second claim from inside the log, and the rule exists for the
cognition whose head is outside it.

Standing is a lifecycle keyed by actor, where the lifecycle above is
keyed by edge kind; the two compose. A render shows standing where it
matters, on a decision or a task, and not on evidence. Silence stays
claimed, never rejected. Confirmation must cost a keystroke or a batch,
or the human stops giving it and the state means nothing.

Who a human contour is stays open. The core has an actor kind for the
human and no identity behind it. Standing per confirmer needs one only
when a second person confirms, and the question waits for that person.

The maps do not split by contour. One map per contour multiplies what a
reader holds for a distinction the fold derives, so the contour is a
view over one map and never a storage boundary. The mixing today is
right; the surface becomes visible when it gets its one missing edge.

This is what makes co-ownership an operation rather than a rule:
agreement with provenance between contours.

The domain is now two modules. `core` holds experience and maps; `harness`
holds the ports a loop needs to drive a model over them - `Model`, `Tool`,
`Snapshot`, and the policy that asks before a tool runs - and depends on
`core`. A Python SDK that folds a map never sees a `Model` trait.

One thing did not move. The core keeps the name `Policy` for the
ask-before-write gate, and the collaboration rule - who may remove what,
an option needs a why, a decision is superseded - stays inside
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
| `percept` binary | CLI and TUI over both. | Now. |
| `percept-wasm` or FFI | The fold for SDKs and the browser. | The first SDK. |

The module split that tested the shape is done: the harness ports are
out of `core`, and it holds no model, no tool, and no tree. A library
target beside the binary is the next step that costs nothing.

Until then the split is a Cargo feature. `app`, `harness`, `tools`,
`code`, `tui`, and `providers` build only under `--features lab`, off
by default, so the binary a developer installs is `core`, `store`,
`mapstore`, and the CLI, and a core change that breaks the lab fails
in the same build.

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
2. Lifecycle on schemas.
3. The surface: the `confirms` and `disputes` edges, standing in the
   fold, and a mark in the render. A `confirm` verb on `percept maps`
   and a `y` on a row in the TUI are the two cheapest ways to give it.

## Recommendation

Validate through the skill direction first. Value has already shown
there, the check costs nothing, and a negative result is about the
core and not the harness. Keep the coding agent as the lab for the one
claim only it can test, the log as environment. Do not judge the core
by how the coding agent codes until the harness is level with the
clients. If the skill direction comes back weak on evidence,
co-ownership, and one log, the core is a memory format with extra
ceremony, and that is worth knowing before a crate split or an SDK.
