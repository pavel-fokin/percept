---
name: code-review
description: Review a completed branch against its agreed outcome, repository rules, and observable behavior. Use before handing back changes with logic or I/O.
---

# Branch review

Read the issue scope, AGENTS.md, and the branch diff against its base.
Trace changed behavior through callers and tests. Use percept's code
map to explore structure.

Check correctness, data loss, attribution, failure handling, and whether
the implementation satisfies the user's outcome. For shared maps, check
that fragment boundaries and evidence remain visible. For hooks, check
client contracts, causation, and isolation across sessions and worktrees.

Report only actionable findings introduced by the branch. Give each a
file location, concrete trigger, consequence, and proposed correction.
Verify suspected failures before asserting them. Fix findings, then run
checks appropriate to the fixes. Keep this review local unless the user
has authorized publishing it.
