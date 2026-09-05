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

1. **Decompose.** Split the request into issues. Order them so each one
   builds on the last. Show the set to the user and get agreement before
   any code.
2. **Record.** Once the set is agreed, write each settled decision into
   the decisions map (see below). Then build.
3. **Build.** An issue with no design left in it, touching one or two
   files, build yourself. Hand anything larger to the
   `software-developer` subagent, one issue at a time.
4. **Review here.** Check the diff against the issue's outcome and
   decisions. `/code-review` and `/simplify` each run once per branch,
   before it merges - not per issue.
5. **Fix.** Small corrections: apply them yourself. Larger rework: send
   it back to `software-developer` with the specifics.
6. **Commit.** Re-run the build and tests yourself first. One commit per
   issue - conventional message, subject line only.

## Recording decisions

The decisions map is percept's record of why. It is rendered to
`.percept/decisions.md`, which AGENTS.md includes, so the next session
starts with it. Every node cites the event it came from.

What to record: a decision the user lives with - a path, a filename, a
flag, a default, a name - settled in this plan. Not where a function
sits or how a rule is coded; those are the builder's, and review
challenges them. The user can say "record this" for anything else.

The source to cite is the user's message that settled it. The
`UserPromptSubmit` hook prints `percept event <id>` into the context
after each prompt; use that id. Without it, the latest prompt is
`~/.percept/bin/percept events search --source claude-code --actor user --size 1`.

One decision, with `P=~/.percept/bin/percept` and `$id` the prompt:

```
$P maps add-node decisions --kind question --name "Where does the log live?" --source $id
$P maps add-node decisions --kind option --name "percept.jsonl in the working directory" --source $id
$P maps add-node decisions --kind option --name "one log under ~/.percept" --source $id
$P maps add-node decisions --kind decision --name "one log under ~/.percept" \
  --prop why="one variable also covers the binary; cross-project search stays free" --source $id
$P maps add-edge decisions --kind resolves \
  --from 'decision:one log under ~/.percept' --to 'question:Where does the log live?' --source $id
```

Name the decision as the option it picks. Add an `evidence` node with a
`contradicts` edge to an option only when the user gave a reason it
lost; skip it otherwise. A node whose name already exists is refused,
so read `.percept/decisions.md` before adding to a question it holds.

At the reflect step, record an approach the session abandoned the same
way: an `evidence` node naming what failed and why, citing the event
where it failed, with a `contradicts` edge to the option it killed.
