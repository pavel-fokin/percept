# AI agent instructions

## Domain

- `Event` is an append-only log entry: `id`, a process-global `seq`, an
  `actor`, an optional `causation_id`, a `created_at`, and a typed
  `payload`. Once committed it never changes.
- `Message` is a value object (no identity) - the shape `Model` needs to
  talk to an LLM. Derived from the log at the boundary, never stored.
- `Actor` (`User`, `Model`, `System`) is the one vocabulary for who a
  message or event is attributed to.
- `Model` is domain-owned, not infrastructure: `percept` needs "a reply
  given messages," never the mechanism behind it.

## Architecture

Layer by dependency direction - each layer depends only on the one below
it, never sideways or up:

| Layer | Package | Owns |
|---|---|---|
| Domain | `percept` | `Event`, `Message`, `Model` - entities and the capabilities they need, as interfaces. Serde-free; depends only on `shared`. |
| Application | `app` | `Conversation` - orchestrates domain objects for one use case, no vocabulary beyond `percept`'s. |
| Presentation | `tui` | Renders the transcript, forwards input. No chat logic of its own. |
| Infrastructure | `providers` | `Stub` today, real LLM clients later - implements `percept::Model`. |
| Infrastructure | `store` | The JSONL event log - the serde boundary - implements `percept::EventLog`. |
| Foundation | `shared` | `Id<T>`, `Timestamp` - value types with no domain meaning. Below the domain; depends only on `uuid`, `jiff`. |

Wire concrete types together only at the entrypoint - `main` in Rust.

## ADR

- 2026-08-29: entity IDs use UUIDv7, each wrapped in a type specific to that entity, not a bare or shared ID type.
- 2026-08-30: Rust is the implementation language, not Go. Both were built
  in parallel to compare the stack; Rust wins going forward. The Go
  implementation is removed - it isn't kept as a reference.
- 2026-08-30: `Event` is an append-only log envelope, not a mutable
  record. Fields: `id` (UUIDv7), `seq` (u64, process-global, gap-free),
  `actor`, `causation_id` (the event that directly caused this one),
  `created_at` (`Timestamp`), and a typed `payload` enum - `message.received`
  is the only variant so far. The domain stays serde-free; the JSON wire
  format lives in `store`, which rejects event types it doesn't know
  rather than the wire event failing to parse. `Id<T>` and
  `Timestamp` live in `shared`, below the domain. Streaming is separate
  from the log: reply chunks reach the UI transiently and one `Event` is
  committed when the reply completes.
- 2026-08-30: events persist to `percept.jsonl` through the domain-owned
  `percept::EventLog` port; `store::Jsonl` is the implementation. The
  path is relative to the working directory, not a fixed location - a
  deliberate choice, so the transcript follows wherever the app is
  launched from. `seq` leaves the process-global static above for one
  counter per `Conversation`, seeded past its own log's maximum and
  advanced only once an append succeeds - one log, one gap-free
  sequence, across both restarts and failed writes. An append failure
  ends the run: losing a committed event silently is worse than
  quitting. One process per log file - nothing enforces this. The file
  is one endless conversation; there's no session or conversation
  boundary yet.

## Workflow

Non-trivial work runs plan, build, review, reflect. A one-line fix
skips it.

- **Plan.** The main agent breaks the request into issues via the `plan`
  skill. An issue has one clear outcome. It is product (a vertical slice
  of behaviour) or tech (refactoring, docs, tooling). Scope each as
  small as it goes. Decisions the user lives with - paths, filenames,
  defaults - are settled with them before the build, never assumed. The
  user agrees the set before any code.
- **Build.** An issue with no design left in it, touching one or two
  files, the main agent builds itself. Anything larger goes to the
  `software-developer` subagent, which follows this file, writes the
  code, runs the build and tests, and reports back. It does not design,
  choose scope, commit, or push.
- **Review.** The main agent checks the diff against the issue, then
  runs `/code-review`. Small fixes land here; larger rework goes back to
  the subagent. `/simplify` runs once per branch, before it merges.
- **Reflect.** Close the session by proposing changes to this workflow.
  Cutting a step counts for more than adding one. Aim for the smallest
  process that still catches mistakes.

The TUI only runs on a real terminal. `scripts/drive.py` forks a pty,
sends timed keystrokes, and prints the frames; `--plain` strips the
escapes so the rendered text can be grepped.

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only. One commit per issue.

## Writing

These rules apply to any human-readable text. Write for a specific reader.

- One idea per sentence. Average under 20 words.
- No metadiscourse. Don't announce what you're about to say.
- Define a term before using it, or link to the definition.
  A reader who doesn't hold the concept won't pick it up.
- Parallel content goes in a table or list. Reasoning stays in
  prose -- lists are for parallel items only.
- Cut before you add. Most sentences fail the question
  "what breaks if this is gone?"
- In code, that means: don't restate what a signature or an
  identifier's name already shows. Comment only what the code can't
  tell the reader.

Target Flesch-Kincaid grade 12 or below. Treat it as a smoke test,
not a gate -- professional terms inflate the score honestly. If
writing scores above grade 12, look at sentence length and clause
nesting first, never at vocabulary. Simplifying words instead of
sentences produces vague prose with a good score, which is the
failure this rule exists to prevent.
