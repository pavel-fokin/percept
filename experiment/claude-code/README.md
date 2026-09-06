# Does a cognitive map change what a Claude Code session does?

`experiment/maps` showed that a decisions map in the prompt replaces the
search that finds a buried fact, and answers questions it was not built
for, on planted facts with percept's own loop. This experiment asks the
same question of a real consumer over real experience: Claude Code
working in this repo, with its own turns recorded in percept's log.

## The assumption under test

A session that starts with the project's decisions map in its prompt,
and can search the log of every earlier session behind it, does three
things better than a session with the hand-written AGENTS.md alone:

- answers "why" questions about the code without searching;
- does not reopen a decision the user already settled;
- does not retry an approach an earlier session abandoned.

The code map failed a different test, "will the model reach for a
tool", because the working tree already had cheap looking in `rg`. The
log and the maps hold what the tree cannot: reasons, rejected options,
open questions, and dead ends. Nothing greps those today.

## Prerequisites

The branch `feat/claude-code-writer` landed: `source` carries name and
path, the log lives under `~/.percept`, hooks record every prompt, tool
call, and reply from Claude Code, maps scope to the project, and each
write rerenders `.percept/<map>.md`. Then one seeding session records
the decisions AGENTS.md and `experiment/maps/README.md` already hold,
through the plan skill's recording path, so every node cites an event.

## Conditions

Every condition runs in a fresh checkout of the same commit, one
headless session per task, `claude -p "<task>" --output-format json`.
The hooks record the session whatever the condition, so measurement is
the same everywhere. Only what the session can read differs.

| Condition | What the session has | Percept analogue |
|---|---|---|
| `bare` | AGENTS.md as it is today, no `.percept/` include | `bare` |
| `render` | AGENTS.md includes `.percept/decisions.md` | `prompt` |
| `search` | no include; the percept skill says to search the log for context | `tool` |

`search` is the control the maps experiment already predicts will lose.
It stays in because it shows whether a Claude Code session reaches for
the log at all when only told it exists.

## Tasks

Mirrors the question kinds of `generalise.sh`, plus one real task. Each
runs three times per condition, since neither the model nor the repo
walk is deterministic.

| Task | Kind | What it tests |
|---|---|---|
| t1 | built for | a "why" a recorded decision answers directly: why `code` is never folded from the log |
| t2 | cross-cutting | which decisions touch the `store` layer, two nodes apart in the map |
| t3 | negative | why an option lost: why tests are not in a top-level `tests/` |
| t4 | open | what is still undecided about map size in the prompt |
| t5 | log only | a side note planted in one earlier session's log, absent from the map |
| t6 | real work | add a small flag whose name, default, or placement a recorded decision already settled |

t5 is the honesty check. A session that treats the render as complete
answers wrong or says it does not know. One that treats it as an index
searches the log and finds the note. t6 is the sharpest: the reply
either applies the settled decision or asks the user to make it again.

## What is measured

Everything comes from the log, filtered to `--source claude-code` and
to the events caused by the task's prompt. One session is one prompt,
so the causation chain is the session.

| Column | How it is read | Meaning |
|---|---|---|
| `correct` | reply matches the task's pattern, as in `run.sh` | the session had the fact |
| `calls` | `tool.called` events caused by the prompt | work done to get there |
| `orient` | Grep, Glob, and Read calls before the first Edit or Write | cost of finding footing |
| `searched` | Bash calls whose command holds `percept events search` | did it go to the log |
| `reasked` | AskUserQuestion calls, or a question in the reply, about a settled decision | a decision reopened |
| `tokens` | `usage` on the headless result, or from the transcript | what the turn cost |

A run directory keeps the JSON result, the reply, and a `trace.txt` of
every tool call, so a number that looks odd has its story next to it.

## Reading the results

| If this happens | It means | Do next |
|---|---|---|
| `render` correct on t1-t4 with no search and fewer `orient` calls than `bare` | the prompt condition transfers from percept's loop to Claude Code | move to the size question below |
| `render` correct but `orient` calls unchanged | the map answers "why", not "where"; expected, it is not a code map | nothing; that is the right division of labour |
| `bare` also correct on t1 and t3 | AGENTS.md already holds the fact at this repo's size | grow the decision set or pick a map AGENTS.md cannot hold, dead ends first |
| `search` never calls percept | told-to-read is dead in Claude Code as in percept | drop read-by-instruction from the skill for good |
| `render` searches on t5 and finds the note | the map is used as an index, not the truth | keep the render whole |
| `render` answers t5 wrong from AGENTS.md | the render reads as complete | the render needs a visible "what this map does not hold" line |
| `reasked` is zero for `render` and non-zero for `bare` on t6 | the sharpest win: settled stays settled | this alone justifies the skill |
| `render` and `bare` match on every column over three runs | the render adds nothing at this size | the assumption is not falsified but not shown either; scale the decision set before concluding |

## After the numbers: two checks on real experience

Both need a few days of ordinary sessions in the log first.

**Noise.** `percept events search --contains <a decision's term>` over
the full log. If the decision surfaces in the preview lines among the
tool events, looking stays cheap on real experience. If it drowns, the
write side is the fix, not search: the plan skill records decisions,
so search is not the path to them anyway.

**Reflect on real sessions.** `percept reflect` over events from
`claude-code`, scored as the first experiment scored it: nodes built,
nodes cited, refusals. This is the honest version of that experiment.
Planted facts were thirty clean messages; a coding session is hundreds
of tool events around a few sentences of reasoning.

## What would settle it

The assumption holds if `render` wins `reasked` and `correct` on t1-t4
and t6 while treating t5 as an index. It fails if `render` matches
`bare` across the board at a decision set of fifty or more nodes. In
between, the result is a size: the number of decisions at which the
render starts to pay, which is the number the native harness will need.
