@AGENTS.md

## Claude Code

What only this client can do; the flow itself is in AGENTS.md.

- The two review passes are `/code-review` for correctness and
  `/simplify` for simplification, each once over the branch.
- The `percept` and `plan` skills under `.claude/skills` are symlinks
  into `.agents/skills`; edit them there.
- `.claude/settings.json` runs `scripts/agent-hook.py claude-code` on
  every prompt, tool use, and stop.
