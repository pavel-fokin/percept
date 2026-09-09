---
name: plan
description: >-
  Break a feature request or body of work into small issues, then drive
  build and review. Use when the work spans more than one file or step.
  Skip it for a one-line or trivial fix - just make the change.
---

# Plan

Turn a request into a short list of issues, each built and then reviewed
here, in the main agent. Larger issues go to the `software-developer`
subagent to build.

## What an issue is

- A task with a **clear, defined outcome**. You can tell when it is done.
- Written in plain language. Short. No term the outcome does not need.
- One of two kinds:
  - **Product** - a vertical slice of product behaviour. It cuts through
    every layer it needs to and leaves one observable change.
  - **Tech** - anything that supports the product: refactoring, docs,
    tooling, dependency moves, test scaffolding.
- Scoped as small as it can be. Two outcomes means two issues.
- Each decision names the choice made, not just the question. Decisions
  the user lives with - paths, filenames, defaults - are settled with
  them before the build, never assumed.

## Issue format

```
## <product|tech>: <outcome, a few words>

<1-3 plain sentences: what changes, and how you know it is done.>

Decisions:
- <decision>: <choice> - <one line why>
(omit this section if there are none)

Touches: <files or areas>
Verify: <how to confirm the outcome>
```

## Steps

1. **Decompose.** Open `percept maps show tasks --format md` first: a
   request may already be an open task, with its why and what it
   waits on. Split the request into issues. Order them so each one
   builds on the last. Show the set
   to the user and get agreement before any code. An explicit request to
   implement a proposal already discussed supplies that agreement; ask
   only about choices still open that the user lives with.
2. **Record.** Once the set is agreed, write each settled decision into
   the decisions map (see below). Then build.
3. **Build.** An issue with no design left in it, touching one or two
   files, build yourself. Hand anything larger to the
   `software-developer` subagent, one issue at a time.
4. **Review here.** Check the diff against the issue's outcome and
   decisions. A correctness pass and a simplification pass each run
   once per branch, before it merges - not per issue - with whatever
   review tooling this client has (AGENTS.md, Review).
5. **Fix.** Small corrections: apply them yourself. Larger rework: send
   it back to `software-developer` with the specifics.
6. **Commit.** Re-run the build and tests yourself first. One commit per
   issue - conventional message, subject line only. An issue that was
   an open task gets an `outcome` on the tasks map naming the commit,
   with a `resolves` edge to it (see the percept skill). Work the
   session found but left undone goes in as a new task, with its why.

## Recording decisions

The decisions map is percept's record of why, folded live from the
log - `percept maps show decisions --format md`, or the bounded
fragment a session start prints - so the next session starts from it
without a committed file to go stale on the wrong branch. Every node
cites the event it came from.

What to record: a decision the user lives with - a path, a filename, a
flag, a default, a name - settled in this plan. Not where a function
sits or how a rule is coded; those are the builder's, and review
challenges them. The user can say "record this" for anything else.

The source to cite is the user's message that settled it. The prompt
hook prints `percept event <id>` into the context after each prompt;
use that id. Without it, the latest prompt from this project is

```
~/.percept/bin/percept events search --source claude-code --actor user \
  | jq -r --arg root "$(dirname "$(git rev-parse --path-format=absolute --git-common-dir)")" \
      'select(.source.path == $root) | .id' | tail -1
```

A Codex session's prompts carry `--source codex` instead. The log is
shared by every project, and `events search` filters by source name
only, so the path filter is what keeps a foreign project's prompt out
of this map's sources. percept stamps every event with the main
checkout's path, worktree or not, which is why the filter asks for the
common git dir's parent and not `--show-toplevel`.

One decision, as one document on stdin, with `$id` the prompt. Every
write carries `--actor model`: the agent is recording, not the user,
and the map shows the difference.

```
~/.percept/bin/percept maps record decisions --actor model --source $id <<'EOF'
question "Where does the log live?"
option "percept.jsonl in the working directory"
  why "one log per checkout makes cross-project search a join"
  answers question
decision "one log under ~/.percept"
  why "one variable also covers the binary; cross-project search stays free"
  resolves question
  cites src/main.rs:40-52
EOF
```

A line at the margin is a node, `kind "name"`. An indented line under
it is a property, `why "..."`, an edge to a short id or to the latest
node of that kind above it, `resolves question`, or `cites
path[:from-to]`, which publishes the text as a `file.cited` event and
adds it to the node's sources. The reply lists what was written, one
short id per line. The document is checked whole before the first
write; a failure after that names the node it reached, so continue
from there rather than repeating. `add-node` and `add-edge` remain
for a single change.

An option is an alternative that lost, and it says why in `why`: the
store refuses one without it. The pick is not an option, it is the
decision, named as the choice it made. When nothing real was rejected,
record the question and the decision alone. Every option gets its
`answers` edge: that edge is what lets `--around` a question reach the
alternatives weighed for it. A name that starts with `--` is passed as
`--name=<name>`, or clap reads it as a flag.

A node whose name already exists is refused, so read `percept maps
show decisions --format md` before adding to a question it holds.
That render shows questions and decisions only; `percept maps show
decisions --around 'question:<name>'` shows a question's options and
evidence, and `--since 1d` shows what the map gained today.

A decision that changes an earlier one is never a removal. Add the new
decision, then point it at the old one:

```
$P add-edge decisions --actor model --kind supersedes \
  --from 'decision:<new>' --to 'decision:<old>' --source $id
```

The old decision leaves the headlines and renders as `was` under its
successor, so a reader who knew it still finds it.

At the reflect step, record an approach the session tried and abandoned
as an option under the question it was trying to answer, its `why`
saying what failed, citing the event where it failed.
