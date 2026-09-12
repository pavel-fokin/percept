@AGENTS.md

## Claude Code

What only this client can do; the flow itself is in AGENTS.md.

- The two review passes are `/code-review` for correctness and
  `/simplify` for simplification, each once over the branch.
- The `plan` skill under `.claude/skills` is a symlink into
  `.agents/skills`; edit it there.
- `.claude/settings.json` runs `percept hook claude-code` on session
  start and every prompt, tool use, and stop, and allows `percept maps` and `percept
  events` without a permission prompt. `percept init claude-code`
  regenerates it.
