# The decisions map under three reader budgets

Branch `worktree-decisions-budgets`, 2026-09-06. This note records
what changed, why, and what was rejected, so the branch can be compared
with another solution to the same concern.

## The concern

The decisions map was growing by three questions, seven options, and
three decisions per planning session, rendered as three kind-sorted
sections plus an edge list. Every node cost the reader the same whether
it mattered today or not, and nothing ever left. Pavel named three
budgets a map spends on its human reader:

| Budget | Question it asks |
|---|---|
| Overview | How many concepts must be held at once to understand one decision? |
| Change | How much new meaning must be absorbed after each update? |
| Verification | How hard is it to check a conclusion and see what a correction touches? |

The change budget matters most. An agent that keeps improving the
structure makes the human lose their landmarks, so stability of the
representation is a value beside accuracy and compactness. Two loops
run over the shared maps, the agent's and the human's, and a map is
neither party's internal understanding. Co-editing is how divergence
gets detected.

## The diagnosis

Granularity was the symptom. Two mechanisms were the cause:

1. The render showed every node, sorted by kind. Options were the bulk
   and carried the least. The questions experiment had shown the model
   answers "why not X" from the decision's own `why` text as often as
   from separate nodes.
2. No node ever left, and no rule said how a decision changes. The only
   tool was removal, which takes a reader's landmark with it.

## The design

Each rule below names the budget it serves.

| Rule | Budget | How |
|---|---|---|
| Stable order | Change | Questions render in first-seen order, grouped under the prompt that raised them. Nothing already shown moves when a node is added. |
| Group by the raising prompt, not the settling one | Change | The raising prompt never changes. The settling prompt moves when a decision is superseded. A decision that cites a different prompt names it on its own `source` line. |
| Supersede, never rewrite | Change, Verification | A correction is a new decision with a `supersedes` edge to the old one. The old node leaves the headlines and renders as a `was` line under its successor. The chain is followed to its end, so a correction needs no new `resolves` edge. |
| User nodes are authoritative | Change | Every node and edge records its actor. The model may attach edges to a user-written node but never remove it, remove a user-written edge, or remove a node that a user-written edge touches. `revise_map` refuses and says to supersede. |
| Options and evidence leave the render | Overview | They stay in the fold and are reached with `--around 'question:<name>'`. An `answers` edge from option to question makes that reachable. |
| `--since` on `maps show` | Change | Lists the nodes added since an instant and the ends of the edges added since. The same values and parser as `events search --since`. Refused for the code map, which has no history. |
| Catalogue line in the prompt | Overview | Each map's header carries its purpose, node and edge count, and last change. The model can tell whether opening a map is worth a call. |
| Model-written nodes are marked | Verification | `(model)` after the name. User and system nodes are unmarked. |

The render on this repo's real log went from about 110 lines in three
sections to about 45 lines in six prompt groups, one line per question
and one per decision.

## What changed, by commit

| Commit | Layer | Change |
|---|---|---|
| stamp map nodes and edges with actor and time | domain, store, cli, code | `Node` and `Edge` gain `actor` and `added_at`, taken from the event on fold. `Map::apply` takes the actor it will be committed under. `maps show` prints both, except for derived maps. |
| add supersedes edge | domain | `SUPERSEDES` edge kind. `Map::headlines` excludes superseded nodes. `Map::successor` and `Map::predecessors` walk the chain. |
| refuse model removal of a user-written node | store | `revise_map` guards; later extended to edges and cascades. |
| add `--since` to maps show | domain, cli | `Map::since`. Runs after `--around`, so it reads as "what changed near this node". |
| carry each map's purpose, size, and last change in the prompt | domain, app, store | `Schema.purpose`. `Map::last_changed`. `maps list` prints the purpose. |
| render decisions as questions grouped by prompt | store, shared | `Id::minted_at` reads the date from the UUIDv7, so a heading needs no log access. `Timestamp::date`. |
| docs | AGENTS.md, plan skill | Domain and workflow text; the recording recipe gains `answers` and `supersedes`. |
| add answers edge | domain, plan skill | Found by running `--around` on the real log: options had no path to their question. |
| fix: settle a question through its supersession chain, guard user edges | domain, store | From the code review, see below. |
| refactor: move the settle query into the domain | domain, store | From the simplify pass. `Map::settled_by` and `Map::settles`; the renderer only iterates. |

## Rejected, and why

Each is recorded in the decisions map as an option with the evidence
against it.

- **Drop `option` as a node kind.** It would orphan the recorded
  options, and hiding them from the render already removes the reader
  cost. Revisit after the questions experiment runs on a flatter
  schema.
- **Flag stale decisions against the code map.** A decision's name is
  prose and a code node's name is a path. There is no join key.
- **Let the model merge or reorganise.** Accuracy and compactness can
  improve without moving landmarks. Where they cannot, the reader
  decides. Nothing in this branch gives the model a merge operation.
- **Group by the settling prompt.** The first render did this by
  accident of the data, and the docs promised it. It was superseded on
  the real map by grouping under the raising prompt, which exercised
  the new path: the question stayed in place and the new decision
  rendered under it with its own `source` and a `was` line.

## What review caught

`/code-review` at high effort confirmed seven defects; all are fixed.

| Defect | Fix |
|---|---|
| A superseded decision's successor did not render under the question; the question showed `open`. | Settling follows the chain to its end. |
| `remove_edge` had no user guard, and removing a model node dropped user-written edges. | Edges carry an actor. Both cases refuse. |
| Grouping was by the raising prompt while the text said settling. | Text fixed; the decision names its prompt when it differs. |
| Per-node `sources:` lines were gone. | A decision citing a different prompt than its group gets a `source` line. |
| A two-hop chain lost the oldest decision. | `predecessors` is transitive. |
| A `resolves` edge between the wrong kinds made prompt and file disagree. | Only decision-to-question edges settle. |
| An options-only map rendered as a truncated file. | It says how many nodes of other kinds it holds. |

`/simplify` moved the settle query into `Map`, folded two supersession
scans onto one helper, made derived-ness one function, and flattened
the renderer's grouping. Skipped: parsing `events search --since` at
clap to match `maps show` (outside the diff), declaring the whole
render shape on `Schema` (one question-shaped map exists), and an
`--actor` flag on the CLI write verbs (unsettled).

## How to verify

From the worktree, with the shared log:

```
cargo test
P="env PERCEPT_HOME=$HOME/.percept ./target/debug/percept"
$P maps show decisions --since 1d | jq -r 'select(.kind) | "\(.actor) \(.kind) \(.name)"'
$P maps show decisions --around "question:How is a decision corrected?" \
  | jq -r 'if .kind then "\(.kind): \(.name)" else "\(.from) \(.edge) \(.to)" end'
cat .percept/decisions.md
```

The render file is committed, so `git diff main -- .percept/decisions.md`
shows the shape change on real data.

## Taken from the Codex branch

Codex answered the same concern on `codex/shared-maps`. Pavel compared
the two and asked for its strongest parts to be folded in here, with
the Claude Code and Codex interop kept whole and the HTML render left
out. What came across, and how:

| From Codex | Here | Changed on the way |
|---|---|---|
| Shared capture hook `scripts/agent-hook.py`, `.claude/settings.json`, `.codex/hooks.json`, one `software-developer` body under `.agents` | Cherry-picked | Nothing. |
| Skills under `.agents/skills`, `.claude/skills` as symlinks, `README.md`, `.gitignore` | Cherry-picked, then rewritten | The plan skill keeps this branch's recording recipe, with Codex's line that an explicit "implement it" supplies agreement. The percept skill is rewritten around questions, `--since`, `supersedes`, and actor marks instead of commitments. |
| `.percept/index.md`, one row per map | Kept | Rows describe this branch's render and entry points. AGENTS.md points at it and keeps the decisions include Codex had removed. |
| `read_map` with `around`, `depth`, `kinds`, and coverage counts | Ported | Lives in the domain as `Selection` and `Fragment` on `Map`, so the CLI and the tool share one cut order: around, since, kinds. `since` is accepted too. The count line has the numbers only; Codex's prose notices are gone. A filtered `maps show` prints the same line on stderr. |

Pavel then fixed the rule behind the interop: the setup must serve any
coding agent, not two. That is recorded as a decision superseding the
two-client one, which was the first real supersession on this map: the
old question now shows the new decision with its own `source` line and
the old decision as `was`, exactly as the render was designed to. The
hook script's fixed list of two client names went with it.

A second round of decisions closed the open items: the plan skill
records as `--actor model`, so the actor mark finally carries
information; `read_map` is offered in every shape; an option is only a
rejected alternative and must say why, enforced in the shared write
path so history still folds; the shared review skills went, and
CLAUDE.md names Claude Code's own; `since` has one parser everywhere.
The main checkout's old shell hooks and local settings are backed up
under `~/.percept/legacy`.

Left on the Codex branch, with the reason:

- **`commitment` nodes, `details` edges, HTML render.** The overview
  was a model's grouping marked `proposed`, written to a temp log, so
  the shared log never held it. The user was not sure about HTML.
- **`PERCEPT_MAPS` default flipped to `tool`, `read_map` always
  registered, reflect prompt rewritten.** Untested against a model on
  this branch; each is a separate decision.
- **Admission rule** ("not one node per flag or filename"). Cuts the
  change budget at the source and is worth deciding. Not taken without
  the user's word, since AGENTS.md says the opposite today.
- **`--markdown` on `maps show`.** No reader asked for it.

## Open

- The plan skill records decisions through the CLI as `user`. An agent
  recording on the user's behalf is attributed to the user. Whether
  that is right for co-ownership is not settled.
- Merging into main meets an untracked `.claude/skills/percept/`
  directory and hook entries in `.claude/settings.local.json` from the
  old setup. Both must go first, or prompts are recorded twice and the
  skill symlink cannot be created. The README says how.
- The read moment per map (open `attempts` on a failure, `decisions` at
  the plan step) is still the model's judgment. Encoding it as a
  trigger percept fires is the next step for the overview budget.
- One workflow change is worth trying: run the changed command once
  against the real log before `/code-review`. Both real render defects
  were found that way, not by tests.
