# tasks

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. Open tasks in the order they were raised, grouped under the prompt that raised them, each with what it waits on. Done and dropped tasks follow under `done`, each with its outcome. A task's own history: `percept maps show tasks --around 'task:<name>'`. What changed lately: `percept maps show tasks --since 1d`.

## 2026-09-07 · 01a07b0e-9c5c-73b0-8434-d21b5f6e0f61
- "tell a coding turn to branch from main, not HEAD" (model): why: "the agent branched from the checked-out feature branch, so its PR carried that branch's commits; a line in AGENTS.md covers it"
- "cancel a streaming turn without quitting the TUI" (model): why: "Esc quits the session; on a fifty-call coding turn the user wants to stop the turn and keep the transcript"
  waits on "make the reply stream cancellable" (model)
- "make the reply stream cancellable" (model): why: "nothing can stop a stream today; a cancel key has nothing to call"
- "let /undo reach the last turn after a restart" (model): why: "the undo point lives in the session; the snapshot ref survives until the next prompt, so a restart forgets what it could still restore"

## done
- "send the project's instructions to a coding turn" (model)
  outcome "done in 1f1a9a9: AGENTS.md goes into the system prompt under PERCEPT_TOOLS=code" (model): ref: "1f1a9a9"
