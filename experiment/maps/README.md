# Does a cognitive map change how the model answers?

percept's claim is that a map makes looking cheap: a fact the model
once judged sits in the prompt as a map node, so a later turn reads it
there instead of searching the log. This experiment plants one fact,
buries it past the model's window, and asks about it with and without
a map.

## Setup

One planted fact, as a user message:

> Decision on ollama connection retries: we weighed 3, 5, and 7
> attempts. 3 gave up while a model was still loading and 5 was
> borderline. We settled on 7 retries, with a two-second pause between
> them.

Then 30 unrelated messages on top, so the fact is outside the 20 events
the model reads as transcript. Then one question:

> How many retries did we settle on for the ollama connection, and why?

A reply counts as correct when it contains `7` or `seven`.

## Conditions

| Condition | What the model has | What it tests |
|---|---|---|
| `bare` | the log only | can it find a buried fact by searching |
| `shell` | a decision map recorded from the shell with `percept maps`, citing the planted event | does a map in the prompt replace the search |
| `reflect` | a map it built itself with `percept reflect`, run while the fact was still in the window | can it build a usable map on its own |

Each condition runs three times, because the model is not
deterministic. Every run starts from an empty log in its own directory.

## Run it

Needs ollama on `localhost:11434` with the model `main.rs` names, and
`jq`.

```
./experiment/maps/run.sh            # three runs per condition
RUNS=1 ./experiment/maps/run.sh     # a quick pass
CONDITIONS="bare shell" RUNS=5 ./experiment/maps/run.sh
```

Expect a few minutes: every run is one model turn, and a reflect run is
two.

## Read the results

The script prints one row per run and a total per condition, and keeps
everything under `runs/<timestamp>/`:

| Column | Meaning |
|---|---|
| `correct` | the reply contains the answer |
| `calls` | tool calls the turn made; zero means the model answered from the prompt |
| `found-seed` | a tool result carried the planted event's id, so the search reached it |
| `nodes` | nodes in the decision map when the question was asked |
| `sources` | nodes citing at least one event, out of nodes built; only a `reflect` map can fall short |
| `time` | wall time of the question's turn |

Per run, the directory holds `percept.jsonl` (the whole log),
`reply.txt`, `trace.txt` (every tool call and result), `map.jsonl`
(`maps show decisions` before the question), and for `reflect` runs
`reflect-reply.txt` and `reflect-trace.txt`.

Things worth reading in a run directory when a number looks odd:

- `trace.txt` shows what the model searched for and what came back.
- `reflect-trace.txt` shows what `revise_map` calls the model made and
  which rule refused them.
- `percept events search --actor model --type thought.recorded --full`
  in the run directory shows the model's reasoning per step.

## What to expect

- `bare` should answer correctly only when it searches and the search
  hits, so `calls` is at least one and `found-seed` is `yes`.
- `shell` should answer correctly with zero calls: the map is in the
  prompt.
- `reflect` shows whether the model can build the map at all. A run
  with zero nodes means every `revise_map` call was refused; the
  reflect trace says why.

## One map, many questions

`generalise.sh` asks whether a map makes reasoning cheap for questions
it was not built around. It plants four decisions, one open question,
and one side note in a fresh log, records the decisions map from the
shell so every condition sees the same map, buries it all, and asks
five questions. Each question runs in its own fresh log, so one
question's searches never sit in the window of the next.

| Question | Kind | What it tests |
|---|---|---|
| q1 | built for | the retries question the map answers directly |
| q2 | cross-cutting | which decisions involve ollama, two nodes apart |
| q3 | negative | why 5 retries lost, an evidence node |
| q4 | open | what is undecided, a question with no resolving edge |
| q5 | log only | whose idea the pause was; the map has no such node |

The side note behind q5 is left out of the map on purpose. A model
that treats the map as complete answers wrong; one that treats it as
an index searches past it.

| Condition | `PERCEPT_MAPS` | What the model has |
|---|---|---|
| `bare` | - | the log only, no map recorded |
| `prompt` | `prompt` | the whole map in the prompt, today's default |
| `headlines` | `headlines` | question and decision nodes in the prompt, `read_map` tool |
| `tool` | `tool` | the map's name and size in the prompt, `read_map` tool |

```
PERCEPT_PROVIDER=openai ./experiment/maps/generalise.sh
RUNS=1 QUESTIONS="q2 q5" CONDITIONS="bare prompt" ./experiment/maps/generalise.sh
```

Besides the columns of `run.sh`, each row carries `reads` (`read_map`
calls) and the turn's tokens summed from its `model.called` events:
`in`, `out`, and `cached`, the prefix the provider served from cache.

## Results, 2026-09-03

gemma4 through ollama, three runs per condition, 30 events of burial.
The bare and shell rows come from `runs/20260903-084730`, the reflect
rows from `runs/20260903-085124`; a ten-minute limit on the first
batch cut it off after one reflect run, so that condition was rerun
alone.

| condition | correct | tool calls per turn | time per turn | map nodes |
|---|---|---|---|---|
| bare | 3/3 | 2 | 25-29s | 0 |
| shell | 3/3 | 0 | 8-11s | 6 |
| reflect | 3/3 | 0 | 9-13s | 5-8 |

What the rows say:

- **Searching works.** Without a map the model found the buried fact
  every time, always by the same two calls: `search_events` on
  `ollama` and `retries`, then `read_event` on the hit. It paid two
  tool calls and roughly three times the wall time for it.
- **A map replaces the search.** With the shell-built map in the
  prompt the model answered from it with no tool call, and its reply
  quoted the map's evidence node rather than the original message.
- **The model can build the map itself.** All three reflect runs
  produced a map that the later turn answered from with no search. The
  maps were bigger than the hand-built one (an evidence node per
  option) and named the question and decision differently each time.
- **Building is the expensive step.** A reflect turn took 41-65
  seconds. One of the three needed a second `revise_map` call, after
  the first was refused for an edge missing its `to`; the reflect run
  in `runs/20260903-084730` needed three, refused twice for integer
  property values where the schema wants strings. An earlier quick
  pass (`runs/20260903-084441`) shows the failure mode: the model put
  the operation name into every node's `kind`, was refused twice,
  built nothing, and the later turn fell back to searching.
- **No sources were cited.** Every reflect-built node has an empty
  `sources` list. The transcript shows no event ids, and the model did
  not search for one before writing the map, so the provenance the
  design asks for is missing whenever the model builds from what is in
  its window. Fixed since: `revise_map` refuses an uncited node and the
  reflect prompt says to search first; `runs/20260903-092102` shows
  5/5 cited.

So on this fact the map does what it is for: an answer from the prompt
instead of a search, at the cost of one slower turn to build it. What
this does not measure is whether a map still helps when the question
is not the one the map was built around, and whether a map kept
current over many reflects stays small enough to send every turn.

## Results, 2026-09-03, gpt-5.6-luna

The same script against OpenAI's gpt-5.6-luna through the Responses
API at reasoning effort `low`, three runs per condition, 30 events of
burial. Rows from `runs/20260903-105139`. Run with
`PERCEPT_PROVIDER=openai`; the script skips the ollama check then.

| condition | correct | tool calls per turn | time per turn | map nodes | sources |
|---|---|---|---|---|---|
| bare | 3/3 | 3 | 12-15s | 0 | - |
| shell | 3/3 | 0 | 2-3s | 6 | 6/6 |
| reflect | 3/3 | 0 | 1-3s | 7 | 7/7 |

A reflect turn took 9-12 seconds, against 41-65 for gemma4.

What differs from gemma4:

- **The first search is wasted on an empty bound.** In every bare run,
  and two of three reflect runs, the first `search_events` call sent
  `since: ""` and was refused with `invalid timestamp`. The model fills
  every field the schema offers, and the schema does not say to omit
  an unused bound. The retry then found the fact with a 500-character
  preview, so no `read_event` followed. Two calls of the three were the
  search; the fix is in the tool, not the model: accept an empty string
  as no bound, or say in the description to omit it.
- **It records the map unasked.** The third call in every bare run was
  `revise_map`: after finding the buried fact, the model wrote the
  question, three options, evidence, and the decision into the
  decisions map, citing the found event, without being told to. The
  `nodes` column reads 0 because the map is captured before the
  question. On gemma4 this never happened.
- **Reflect builds a cited map first time.** All three reflect runs
  searched once, then made one `revise_map` call that went through
  whole: seven nodes, every one citing the planted event, no refusals.
  One reply claimed the map "already held everything" while its trace
  shows it had just added seven nodes.
- **Same shape of result, ten times faster.** The map replaces the
  search here as it did on gemma4: three calls and 12-15 seconds
  without it, none and 2-3 seconds with it.

## Results, 2026-09-03, one map, many questions

gpt-5.6-luna at reasoning effort `low`, three runs per cell, 30
events of burial, from `runs/20260903-113242-generalise`. Every
condition but `bare` had the same 30-node shell-built map.

| condition | correct | calls per question | tokens in per question | tokens out | time per question |
|---|---|---|---|---|---|
| bare | 13/15 | 1.7 | 5700 | 570 | 3-36s, mostly 5-11s |
| prompt | 15/15 | 0.3 | 2500 | 120 | 1-4s |
| headlines | 15/15 | 1.1 | 3900 | 130 | 2-11s |
| tool | 15/15 | 1.3 | 4200 | 130 | 3-9s |

Per question, calls summed over three runs:

| question | bare | prompt | headlines | tool |
|---|---|---|---|---|
| q1 built for | 7 | 0 | 4 | 3 |
| q2 cross-cutting | 4 | 0 | 3 | 3 |
| q3 negative | 6 | 1 | 3 | 5 |
| q4 open | 4 | 0 | 3 | 3 |
| q5 log only | 5 | 3 | 3 | 5 |

The `cached` column was zero in every cell, including the second
call of a turn seconds after the first with the same prefix. Either
the model does not report it or the prefix never qualifies; not
resolved here.

What the rows say:

- **The map answers questions it was not built for.** Cross-cutting,
  negative and open questions all came back right from the prompt
  with no call, in one to four seconds. This is the result the first
  experiment could not give: the representation is not a cache of one
  answer.
- **The model treats the map as an index, not as the truth.** On q5,
  whose answer the map does not hold, the prompt condition searched
  the log in every run and found the side note. On q3, one prompt run
  searched to confirm the evidence node before answering. So the
  thinner shell answer seen in the first experiment is not the rule:
  when the map is visibly silent, the model goes to the log.
- **Bare fails by not looking.** Both bare misses came without a
  search or with a search that returned the wrong subset. One q4 run
  made no call and answered "nothing is undecided" from the twenty
  chatter notes in its window. One q2 run is a scoring miss: the reply
  named the two decisions that mention ollama, and the pattern wanted
  a word it did not use; the pattern is fixed for later runs.
- **Headlines and tool cost one call and gain little over prompt.**
  Both were right on every question, but paid a `read_map` call on
  every map question, and on q5 headlines went straight to search
  while tool read the map first in two of three runs. Tokens in were
  1.5 to 1.7 times prompt's, because a turn with a call replays the
  whole prompt twice.
- **The whole map is cheaper than the search that replaces it.** A
  prompt turn with the 30-node map cost about 1900 tokens in. A bare
  turn cost about 3300 per model call and usually made two. The map
  pays for itself on the first question, at this size.
- **Bare builds the map when nobody asked.** As in the first
  experiment, a bare run recorded what it found with `revise_map`
  before answering.

Where the ceiling is, this run does not say. Thirty nodes cost a few
hundred tokens. A map that no longer fits the prompt is the point at
which `headlines` earns its place, and that is a size question the
next experiment should set up on purpose.

## What this means for the idea

The loop the Purpose section draws closes: experience went into the
log, the model built a map from it, and a later turn reasoned from the
map instead of from the experience. On a small local model, with no
prompt engineering beyond the schema's kinds and one tool description.
That is the architecture working end to end, once.

Three things the numbers say about the idea, as opposed to the code:

- **A representation is what makes looking cheap.** Search cost two
  calls and three times the wall time, every time; the map cost none.
  The premise that percept should make looking cheap and leave judging
  to the model holds, and the map is the first thing that made a look
  cheaper than a search.
- **The cognitive history is not yet honest.** Every node the model
  built cites nothing. The design says a cognitive commit points at
  the experience it was drawn from; today the model builds from what is
  in its window, where no event has an id, so it cannot. A map without
  sources is a summary, and a summary cannot be checked.
- **Building is where the cost and the brittleness live.** A reflect
  turn is five times slower than answering from the map, and a small
  model wastes calls on schema mistakes before it gets one through.
  The harness's part worked: each refusal named the rule, and the model
  corrected itself. That is the primitive doing its job - the model
  judges, percept only says no precisely.

What this run does not tell you is the thing the idea most depends on:
whether a map makes *other* reasoning cheap. The question here was the
one the map was built around. A representation earns its place when it
answers questions nobody had in mind when it was built - "why not 5",
"which decisions touch ollama", "what is still open". Until that is
measured, the map is a cache of one answer.

## Recommended next step

Run the generalisation experiment, and fix provenance on the way in.

1. **Provenance first, because it is small.** Either show event ids in
   the transcript the model reads, or have the reflect prompt require a
   search before any `revise_map` call. Measure it with the `sources`
   column this script does not yet have: nodes with at least one
   source, out of nodes built.
2. **Then: one map, many questions.** Plant three or four decisions in
   one log, reflect once, bury, and ask a set of questions the map was
   not written for - a cross-cutting one, a negative one, an open one -
   against `bare`. Score correctness and calls as here. If the map wins
   on questions it was not built around, the idea holds and the next
   constraint is map size in the prompt. If it only wins on the
   question it was built for, the representation is the wrong shape,
   and that is the finding to take to the other schemas.

`node.updated` can wait for this. It matters once the model revises a
map it built earlier, and that is what the second experiment will make
it do.
