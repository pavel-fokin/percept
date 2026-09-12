@AGENTS.md

## Claude Code

What only this client can do; the flow itself is in AGENTS.md.

- The two review passes are `/code-review` for correctness and
  `/simplify` for simplification, each once over the branch.
- The `plan` skill under `.claude/skills` is a symlink into
  `.agents/skills`; edit it there.
