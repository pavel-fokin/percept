# Do cognitive rails help across changing requirements?

This experiment gives fresh Codex sessions a small browser chat to build.
The app uses a Node HTTP backend, browser HTML and JavaScript, and a
deterministic fake token generator. It has no package dependencies or model
API. Codex itself still uses the model configured in the CLI.

The hypothesis is that a cognitive record helps a new session preserve the
right design history when a requirement reverses. Rails should improve the
stage 2 and 3 behavior checks without changing stage 1.

## Conditions

Each condition and run starts from the same `seed/` directory in a new Git
repository. The task prompts and behavior tests are byte-identical.

| Condition | Added to the seed |
|---|---|
| `bare` | Nothing. Codex receives the task and repository files. |
| `rails` | `percept init codex --capture`, the cognitive-record instructions in `stages/rails-AGENTS.md`, and one isolated `PERCEPT_HOME` that persists across the three sessions. |

Every task uses `codex exec --ephemeral`, so it starts a fresh Codex session
over the previous task's committed working tree. The rails session can recover
earlier reasoning only from the repository and percept's accumulated log and
maps.

## Tasks and checks

The runner installs only the current task's test before starting its session.
It restores that test from the experiment before scoring, so an agent cannot
make the check pass by editing it.

| Task | Requirement change | Objective check |
|---|---|---|
| 1 | Generation belongs to one connected browser stream. | The stream receives every fake token; disconnect stops and removes the generation. |
| 2 | Generation must survive disconnect and resume. | Reconnect replays the complete ordered token sequence and completion. |
| 3 | Two tabs share a generation; cancellation affects both. | Resume still passes; both streams see the same tokens and cancellation. |

This sequence reverses the ownership decision from task 1, then adds shared
observers. Later requirements never appear in an earlier session.

The score covers the backend generation lifecycle. Serving the browser app
and issuing its cancel request are smoke checks. The saved client diff remains
part of review, but a passing row does not claim full browser UI correctness.

## Run

Prerequisites are Node 18 or newer, npm, Git, an authenticated Codex CLI, and
a Rust toolchain. The runner builds percept in release mode for the rails
condition. `PERCEPT_BIN` can select an existing binary.

Long runs should be detached as required for experiments in this repository:

```sh
mkdir -p experiment/chat-rails/runs
nohup ./experiment/chat-rails/run.sh \
  >experiment/chat-rails/runs/latest.log 2>&1 &
```

The defaults run each condition once. Useful controls are:

```sh
RUNS=3 MODEL=your-model ./experiment/chat-rails/run.sh
CONDITIONS=bare ./experiment/chat-rails/run.sh
CONDITIONS=rails PERCEPT_BIN=./target/release/percept ./experiment/chat-rails/run.sh
OUT=/tmp/chat-rails ./experiment/chat-rails/run.sh
```

`CODEX_BIN` selects an alternate Codex executable. An unknown condition,
invalid run count, existing output directory, or missing prerequisite stops
before any agent session starts.

Run the free local fixture check separately:

```sh
./experiment/chat-rails/check.sh
```

It checks shell syntax, proves a reference server satisfies each staged test,
and proves the seed does not already satisfy task 1. It opens only a loopback
HTTP listener and does not invoke Codex.

## Results

`runs/<timestamp>/summary.tsv` reports the Codex exit status, passed and failed
behavior checks, elapsed seconds, and accumulated event count for every task.
Each task directory keeps the prompt, Codex JSON event stream, stderr, final
message, TAP output, status, and diff. A rails task also keeps the raw percept
log plus the decisions, concepts, and map catalogue as they stood after that
session.

Compare pass rates by task across several runs. A higher rails pass rate on
tasks 2 or 3 supports the hypothesis. Equal pass rates give no evidence that
the record helped at this project size. A lower rate rejects the intervention
as configured. Use the saved diffs and maps to explain a difference; event
count and elapsed time are costs, not success measures.

The two conditions cannot separate the map from the instructions that cause
Codex to maintain it. That pair is the cognitive-rails intervention being
tested. Model nondeterminism also makes a single run illustrative rather than
conclusive.
