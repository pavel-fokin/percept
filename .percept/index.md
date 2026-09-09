# Map directory

Choose a map by the question you need answered. `percept maps list`
prints each map's purpose and current size.

| Map | Use it for | Origin | Entry point |
|---|---|---|---|
| decisions | Why a choice was made: the question, the options weighed, the decision that settles it now, and the ones it superseded. | Cognitive commits in this project's event log. | `maps show decisions --format md`, then `--around 'question:<name>'`. `--since 1d` for what changed since yesterday. |
| tasks | What is left to do, why it matters, and what it waits on: open tasks first, done ones below with their outcome. | Cognitive commits in this project's event log. | `maps show tasks --format md`, then `--around 'task:<name>'` for a task's blockers and outcome. |
| code | Files, the symbols they define, what imports what. | The current working tree, rebuilt on every query. | `maps show code --around file:src/main.rs --kind file`, or `read_map` with `map` `code` in a turn that has it. |
| ideas | A candidate worth doing that nobody has committed to yet - not a task, which is already committed and just waiting. | Cognitive commits in this project's event log. | `maps show ideas --format md`, then `--around 'idea:<name>'`. |

A new map is a TOML file at `schemas/<name>.toml` beside this index;
the [percept skill](../.agents/skills/percept/SKILL.md) shows the shape.
Add its row here.

A claim missing from a map may still be in the log. A fragment cut with
`--around` stops at its edge; the stderr line says how much it left out,
and a question's options are one hop from it. Code says what is, not
why.

The shared [percept skill](../.agents/skills/percept/SKILL.md) covers
selecting a fragment, checking a claim against the log, and revising
without moving what a reader has seen.
