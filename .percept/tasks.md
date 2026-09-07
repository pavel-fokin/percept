# tasks

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. A `## contents` list, then every open task at `##` in the order it was raised, each with why it matters and what it waits on. Done and dropped tasks follow under `done`, each with its outcome. A task's own history: `percept maps show tasks --around 'task:<name>'`. What changed lately: `percept maps show tasks --since 1d`.

## contents
- "tell a coding turn to branch from main, not HEAD" (model) · 2026-09-07
- "cancel a streaming turn without quitting the TUI" (model) · 2026-09-07
- "make the reply stream cancellable" (model) · 2026-09-07
- "let /undo reach the last turn after a restart" (model) · 2026-09-07
- "make the map renderer schema-driven, not dispatched on map name" (model) · 2026-09-07
- "default the TUI to code tools, not maps-only" (model) · 2026-09-07

## "tell a coding turn to branch from main, not HEAD" (model)

why: "the agent branched from the checked-out feature branch, so its PR carried that branch's commits; a line in AGENTS.md covers it"

## "cancel a streaming turn without quitting the TUI" (model)

why: "Esc quits the session; on a fifty-call coding turn the user wants to stop the turn and keep the transcript"
waits on "make the reply stream cancellable" (model)

## "make the reply stream cancellable" (model)

why: "nothing can stop a stream today; a cancel key has nothing to call"

## "let /undo reach the last turn after a restart" (model)

why: "the undo point lives in the session; the snapshot ref survives until the next prompt, so a restart forgets what it could still restore"

## "make the map renderer schema-driven, not dispatched on map name" (model)

why: "markdown() branches on schema.name for decisions/tasks and falls back to push_by_kind; a third log-folded schema that wants the settlement-style render would need a new branch. A RenderStyle on Schema, plus the edge roles it needs, would let a new map pick a style without touching the renderer. Deferred: only two schemas today, both one style."

## "default the TUI to code tools, not maps-only" (model)

why: "A TUI session that means to change code silently has no file tools unless the user exports PERCEPT_TOOLS=code, and they only find out mid-turn. Decision 'How are the coding tools switched on?' weighed always-on and rejected it for reflect and map-Q&A turns; this would supersede it by splitting the default per client - TUI to code, CLI stays maps."

## done
- "send the project's instructions to a coding turn" (model)
  outcome "done in 1f1a9a9: AGENTS.md goes into the system prompt under PERCEPT_TOOLS=code" (model)
  ref: "1f1a9a9"
