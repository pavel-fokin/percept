# Does a map answer questions it was not built for?

The first experiment (`../maps`) showed a map replacing a search, but
the question asked was the one the map was built around. A
representation earns its place when it answers questions nobody had
in mind when building it. This experiment plants four decisions, lets
the model build one map from them, buries them, and asks five
questions the map was not written around, with and without the map.

## Setup

Four planted facts, as user messages: three decisions and one open
question. Each names the options weighed and the evidence against the
losers.

| Fact | Decided |
|---|---|
| ollama connection retries: 3, 5, or 7 | 7, with a two-second pause; 3 gave up during model load, 5 was borderline |
| log format: SQLite or JSONL | JSONL; SQLite needs a migration per event kind and cannot be grepped |
| TUI library: cursive or ratatui | ratatui; cursive owns the event loop and fights async |
| capping the map in the prompt: no cap, by node count, by token budget | nothing yet; no evidence on map size |

Then 30 unrelated messages, so every fact is outside the 20 events the
model reads as transcript. Then the questions, each asked in its own
copy of the log:

| Question | Kind | Correct when the reply contains |
|---|---|---|
| How many retries did we settle on for the ollama connection? | direct control, the first experiment's question | `7` |
| List every decision we have taken so far, one line each. | cross-cutting: three facts at once | `7`, `jsonl`, and `ratatui` |
| Why did we not go with 5 retries? | negative: a rejected option | `borderline` |
| Why did we reject SQLite for the log? | negative: a rejected option | `migration` or `grep` |
| Which question is still undecided? | open: a question with no decision | `map`, and `cap`, `size`, or `limit` |

## Conditions

| Condition | What the model has | What it tests |
|---|---|---|
| `bare` | the log only | can search answer questions that span several buried facts |
| `reflect` | a map it built itself with `percept reflect`, run while the facts were in the window | does a map built for no particular question still answer these |

Each condition runs three times. A reflect that builds nothing is
retried, up to three tries, each from a freshly planted log: the model
sometimes plans the whole `revise_map` call in its thinking and then
ends its turn without making it, or claims in its reply to have
written what it never did, and a retry in the same log would read
that claim and believe it.

## Run it

Needs ollama on `localhost:11434` with the model `main.rs` names, and
`jq`.

```
./experiment/questions/run.sh              # three runs per condition
RUNS=1 ./experiment/questions/run.sh       # a quick pass
CONDITIONS=reflect ./experiment/questions/run.sh
```

Expect ten to fifteen minutes: thirty question turns, and a reflect
turn per reflect run.

## Read the results

The script prints one row per question per run, then per condition a
total and a per-question score, then one line per reflect run. It
keeps everything under `runs/<timestamp>/`:

| Column | Meaning |
|---|---|
| `correct` | the reply contains every required term |
| `calls` | tool calls the turn made; zero means the model answered from the prompt |
| `nodes` | nodes in the decision map when the questions were asked |
| `time` | wall time of the question's turn |

Per run, `base/` holds the prepared log, `map.jsonl`, and for reflect
runs `reflect.txt` (tries, seconds, nodes, and how many cite a
source) with a reply and trace per try. Each question's directory
holds its own `percept.jsonl`, `reply.txt`, and `trace.txt`.

## What to expect

- `bare` should get the direct and negative questions by searching,
  and struggle on the cross-cutting one, which needs three separate
  finds inside a five-call budget.
- `reflect` answering the cross-cutting and open questions with zero
  calls is the result the idea needs. If it only gets the direct
  question, the map is a cache of one answer and the shape of the
  `decisions` schema is the thing to revisit.

## Results, 2026-09-03

gemma4 through ollama with a 16k context, three runs per condition,
30 events of burial. The bare rows come from `runs/20260903-092227`,
the reflect rows from `runs/20260903-092723`; a ten-minute limit on the
first batch cut it off during the first reflect run, so that condition
was rerun alone. Reflect run 2 built no map in three tries, so its
five questions ran against an empty map; the last column keeps only
the two runs that had one.

| question | bare | reflect, all runs | reflect, runs with a map |
|---|---|---|---|
| direct | 3/3 | 3/3 | 2/2 |
| cross-cutting | 0/3 | 2/3 | 2/2 |
| negative, 5 retries | 1/3 | 2/3 | 1/2 |
| negative, SQLite | 3/3 | 2/3 | 2/2 |
| open | 0/3 | 2/3 | 2/2 |
| total | 7/15 | 11/15 | 9/10 |
| tool calls | 14 | 4 | 1 |
| time per correct answer | 17-38s | | 6-14s |

What the rows say:

- **A map answers questions it was not built for.** The cross-cutting
  and open questions, which bare got wrong every time, came back
  right from every map the model built, with no tool call and in a
  third of the time. This is the result the idea needed: the map is
  not a cache of one answer.
- **A map answers only what it kept.** The model never built the
  graph the schema describes. Both maps are one `decision` node per
  fact, with the options and evidence folded into properties and no
  edges. Run 1 kept the evidence and answered all five. Run 3 kept
  only the decision, so on "why not 5 retries" the model said the map
  records the decision but not the reasoning, named the search it
  should run, and did not run it. What a map holds is the builder's
  call, and the schema did not steer it.
- **An empty map is not neutral.** Every bare miss on the
  cross-cutting and open questions came with zero tool calls: the
  reply says no decisions have been recorded in the map. The prompt
  says the map is "built from this log", so an empty one reads as "the
  log holds no decisions", and the model stops looking. The same
  happened to reflect run 2. The first experiment did not see this
  because its question named a specific thing to search for.
- **Building is still the unreliable step.** Two runs built a cited
  map on the first try, in 56s and 91s. Run 2 failed three times in
  three ways: a node kind `node`, refused; a search followed by a
  reply claiming the map was updated, with no call; then a call with
  an empty change list. The replies of the failed tries stayed in the
  log, so each retry read "I have updated the decisions map" from its
  predecessor and believed it over the empty map in the prompt. A
  retry in the same log makes the next try worse, not better.
- **Provenance holds.** Every node built cites the event it came
  from, 8 of 8, since `revise_map` refuses an uncited node and the
  reflect prompt says to search first.
- **The context ceiling is the transcript, not the map.** The reflect
  retries reached 15k of the 16k tokens ollama now allows, because
  each search result replays whole in later turns. A four-node map
  costs almost nothing beside that.

## What this means for the idea

The claim survives its first real test: a representation the model
built for no particular question made two kinds of question cheap
that search could not answer at all. Search fails on "list every
decision" not because the events are hard to find but because the
model does not go looking, and a map removes the need to.

Two things the run says about the design:

- The map the model builds is a table, not a graph. Nodes with
  free-text properties carried the whole result; the edge kinds went
  unused. Either the `decisions` schema is asking for more structure
  than the model or the questions need, or the reflect prompt has to
  ask for it. The negative question is the one that tells them apart:
  it needs the evidence kept somewhere, and only the schema-shaped map
  guarantees where.
- The model's own reply is not evidence of anything. It claimed to
  have updated a map it had not touched, and later turns trusted the
  claim. The map, folded from the log, was right; the transcript was
  wrong. percept already treats the map as the source of truth; the
  prompt should say so, and an empty map must not read as an empty
  log.

## Recommended next step

1. **Reword the empty map**, so the prompt says the map holds nothing
   yet and the log may still, or leave an empty map out. Rerun `bare`
   to see whether the model then searches on the cross-cutting and
   open questions. Small change, and it removes the bare condition's
   biggest handicap, which the current comparison flatters the map
   with.
2. **Retry a failed reflect from a fresh log**, in the script, so a
   retry never reads a false claim of success.
3. **Make the reflect prompt ask for the schema's shape** - a
   `question`, its `option`s, the `evidence` for and against, the
   `decision`, joined by edges - and rerun. If the negative questions
   then hold at 3/3, the schema earns its edges. If the model still
   answers as well from a flat map, the schema is more than the
   decisions map needs, and that is worth knowing before adding the
   other kinds.

## Results, second run, 2026-09-03

Same setup, after three changes: the empty map's text in the prompt
now says the log may still hold what the map does not; a failed
reflect retries from a fresh log; and the reflect prompt asks for the
schema's shape - a node per question, option, evidence, and decision,
joined by edges. `runs/20260903-094312`, both conditions in one batch.
Reflect run 1 built nothing in three tries, so as before the last
column keeps the runs that had a map. Run 2's open answer, "Map size
limit", is right and was scored wrong by a term that wanted the word
"cap"; the term is widened now and the table counts it correct.

| question | bare | reflect, all runs | reflect, runs with a map |
|---|---|---|---|
| direct | 3/3 | 3/3 | 2/2 |
| cross-cutting | 0/3 | 2/3 | 2/2 |
| negative, 5 retries | 0/3 | 1/3 | 1/2 |
| negative, SQLite | 3/3 | 3/3 | 2/2 |
| open | 0/3 | 2/3 | 2/2 |
| total | 6/15 | 11/15 | 9/10 |
| tool calls | 13 | 3 | 0 |

What changed and what did not:

- **The empty-map wording changed nothing.** Bare still answered the
  cross-cutting and open questions with zero calls and "no decisions
  have been recorded in the map", 0/6 as before. The model reads the
  map's emptiness, not its caption. Leaving an empty map out of the
  prompt is the remaining option, at the cost of the kinds header the
  first experiment showed a writer needs.
- **The schema's shape is buildable, and it costs.** Two of three
  runs built a schema-shaped map, 19 and 25 nodes, every one cited,
  with edges. A reflect turn now takes four to seven minutes against
  one to one and a half before, because the batch grew four-fold, and
  a bigger batch has more places for one slip - a missing `op`, an
  edge naming a node that is not there - and any slip refuses the
  whole batch. Run 1 lost all three tries that way.
- **Evidence is what the model drops.** Run 2 kept seven evidence
  nodes and answered the negative question from them. Run 3, asked
  for the same shape, built questions, options, and decisions and no
  evidence at all, and the negative question failed the same way it
  did against a flat map. The prompt asks for evidence; the model
  keeps it about half the time.
- **Everything else holds.** Cross-cutting and open questions: right
  from every map, zero calls, 9-11s against bare's 20-30s searches
  that did not happen. The SQLite question, whose evidence is in the
  decision's own name, is right under every condition.

The shape prompt was reverted after this run: the same answers from a
fifth of the map in a quarter of the build time, and the map is paid
for on every later turn. The search-first sentence stays. It is worth
trying again on a larger model.

## What the two runs say together

Across both runs, six maps were built and every question the map
held was answered from it without a search. The idea's claim -
a map makes a class of reasoning cheap that search does not reach -
held on each of them. What the map holds is the open problem, and it
has two halves:

- **Building.** A small model builds a usable map two times in three
  and fails the third in a way no rule catches, because the failure is
  a plan it never carries out or a claim it never made true. Bigger
  batches fail more. All-or-nothing keeps the map consistent and makes
  one slip expensive.
- **Keeping the evidence.** The negative question is the only one
  that separates a map from a list of decisions, and it is the one the
  model fails, under both prompts, by not writing the evidence down.

## Recommended next step

Both halves look like small-model behaviour: trusting an empty map
over a search, planning a call without making it, dropping half of
what the prompt asked for. Before changing the harness to work around
them, run this experiment once against a production-size model.
The provider takes a model name and an endpoint, so the change is one
constant. If a larger model builds the shape reliably and keeps the
evidence, the harness is right and the constraint is the model. If it
fails the same way, the next changes are in percept:

1. Leave an empty map out of the prompt, or send the kinds only when
   the model can write, and rerun `bare`.
2. Let `revise_map` commit the changes before the first refused one,
   so a slip costs one change, not the batch.
3. Ask for less in one reflect - one question per turn - so a batch
   stays small enough to get through.
