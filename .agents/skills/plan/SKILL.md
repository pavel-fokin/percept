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
2. **Build.** An issue with no design left in it, touching one or two
   files, build yourself. Hand anything larger to the
   `software-developer` subagent, one issue at a time.
3. **Review here.** Check the diff against the issue's outcome and
   decisions. Then run a code-review pass for bugs. Run a simplification
   pass once per branch, before it merges - not per issue.
4. **Fix.** Small corrections: apply them yourself. Larger rework: send
   it back to `software-developer` with the specifics.
5. **Commit.** Re-run the build and tests yourself first. One commit per
   issue - conventional message, subject line only.
