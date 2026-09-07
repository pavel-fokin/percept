# tasks

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. A `## contents` list, then every open task at `##` in the order it was raised, each with why it matters and what it waits on. Done and dropped tasks follow under `done`, each with its outcome. A task's own history: `percept maps show tasks --around 'task:<name>'`. What changed lately: `percept maps show tasks --since 1d`.

## contents
- "tell a coding turn to branch from main, not HEAD" (model) · 2026-09-07
- "cancel a streaming turn without quitting the TUI" (model) · 2026-09-07
- "make the reply stream cancellable" (model) · 2026-09-07
- "let /undo reach the last turn after a restart" (model) · 2026-09-07
- "make the map renderer schema-driven, not dispatched on map name" (model) · 2026-09-07
- "score suggestion segments by rarity, not a flat count" (model) · 2026-09-07
- "stop committing the rendered map Markdown, or scope its rewrite to the branch" (model) · 2026-09-07
- "check on a real coding turn that cached tokens rise across rounds" (model) · 2026-09-07
- "say in AGENTS.md that a reply promising to proceed, with no edit in that turn, is a false report" (model) · 2026-09-07

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

## "score suggestion segments by rarity, not a flat count" (model)

why: "suggestions_for uses a floor of 2 shared name segments to tell signal from noise, then a second is_path_like gate (name has slash or colons) lets a lone specific segment like providers cross kinds without letting a prose word like Rust do the same - two heuristics cancelling each other. Scoring each shared segment by how few nodes carry it makes providers strong and the weak on its own, dropping both the 1-vs-2 split and is_path_like."

## "stop committing the rendered map Markdown, or scope its rewrite to the branch" (model)

why: "the render folds the whole shared log, so a percept maps write from any checkout rewrites .percept/decisions.md for whatever branch is out - this session dirtied a foreign feature branch, then its worktree regen pulled in an unmerged branch's decision. Third session to hit decisions-render-vs-shared-log. Options: regenerate on demand (a hook or make target) instead of committing it, or a pre-commit regen, or teach the renderer a branch scope."

## "check on a real coding turn that cached tokens rise across rounds" (model)

why: "the harness branch orders the request stable first and sizes history in steps so a provider can reuse the prefix; tests prove the prefix is identical, only /context on a live OpenAI turn shows whether cached_tokens follows"

## "say in AGENTS.md that a reply promising to proceed, with no edit in that turn, is a false report" (model)

why: "the session the handoff reviewed ended a build turn with 'proceeding' and no code; the harness fix keeps the plan in view, the workflow text still does not name the failure"

## done
- "send the project's instructions to a coding turn" (model)
  outcome "done in 1f1a9a9: AGENTS.md goes into the system prompt under PERCEPT_TOOLS=code" (model)
  ref: "1f1a9a9"
- "default the TUI to code tools, not maps-only" (model)
  outcome "done in 9402950" (model)
  ref: "9402950"
  why: "TUI defaults to the code toolset; headless stays maps; PERCEPT_TOOLS overrides either"
