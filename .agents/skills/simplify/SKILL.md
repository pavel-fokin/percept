---
name: simplify
description: Review a completed implementation for unnecessary complexity while preserving its approved behavior. Use once over a branch after correctness review.
---

# Simplification review

Read the whole branch diff. Look for duplicated rules, unnecessary
abstractions, redundant state, and tests that merely repeat the code.
Prefer existing domain capabilities and client adapters over parallel
implementations of the same rule.

Remove complexity only when the resulting behavior remains clear and
correct. Preserve evidence, failure semantics, and the user's familiar
references. Do not reorganize a cognitive map merely to make it smaller.

Apply worthwhile simplifications and rerun affected checks. Report any
remaining tradeoff that matters to the user. No change is a valid result.
