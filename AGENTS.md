# AI agent instructions

## Purpose

percept records what happens across the tools its user works in, so a
model can query that record. Today it is a console utility: it appends
events and prints them for something else to filter. The ambition is a
harness - percept hosting the loop rather than feeding one - if the
primitive earns it. The shape comes from Recursive Language Models
(arxiv.org/abs/2512.24601), where a model holds a corpus as an
environment and writes programs over it instead of reading it as prompt
text.

Two rules follow from being a primitive. percept does not rank,
summarise, or answer - the model judges relevance, and percept's job is
to make looking cheap. Output is constant-size per event by default, so
a caller spends tokens on a payload deliberately rather than by
accident.

## Domain

- `Event` is an append-only log entry: `id`, an `actor`, a `source`, an
  optional `causation_id`, a `created_at`, and a typed `payload`. Once
  committed it never changes.
- `source` names the writer that produced an event - `tui`,
  `claude-code`, `telegram`. Open by design, where `Actor` is closed.
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
| Application | `app` | `App` - orchestrates domain objects for one use case, no vocabulary beyond `percept`'s. |
| Presentation | `tui` | Renders the transcript, forwards input. No chat logic of its own. |
| Presentation | `cli` | `percept events publish` - appends one event without opening the TUI. |
| Infrastructure | `providers` | `Stub` today, real LLM clients later - implements `percept::Model`. |
| Infrastructure | `store` | The JSONL event log - the serde boundary - implements `percept::EventLog`. |
| Foundation | `shared` | `Id<T>`, `Timestamp` - value types with no domain meaning. Below the domain; depends only on `uuid`, `jiff`. |

Wire concrete types together only at the entrypoint - `main` in Rust.

## ADR

- 2026-08-29: entity IDs use UUIDv7, each wrapped in a type specific to that entity, not a bare or shared ID type.
- 2026-08-30: Rust is the implementation language, not Go. Both were built
  in parallel to compare the stack; Rust wins going forward. The Go
  implementation is removed - it isn't kept as a reference.
- 2026-08-30 (the `seq` parts superseded 2026-08-31): `Event` is an
  append-only log envelope, not a mutable
  record. Fields: `id` (UUIDv7), `seq` (u64, process-global, gap-free),
  `actor`, `causation_id` (the event that directly caused this one),
  `created_at` (`Timestamp`), and a typed `payload` enum - `message.received`
  is the only variant so far. The domain stays serde-free; the JSON wire
  format lives in `store`, which rejects event types it doesn't know
  rather than the wire event failing to parse. `Id<T>` and
  `Timestamp` live in `shared`, below the domain. Streaming is separate
  from the log: reply chunks reach the UI transiently and one `Event` is
  committed when the reply completes.
- 2026-08-30 (the `seq` parts superseded 2026-08-31): events persist to
  `percept.jsonl` through the domain-owned
  `percept::EventLog` port; `store::Jsonl` is the implementation. The
  path is relative to the working directory, not a fixed location - a
  deliberate choice, so the transcript follows wherever the app is
  launched from. `seq` leaves the process-global static above for one
  counter per `App`, seeded past its own log's maximum and
  advanced only once an append succeeds - one log, one gap-free
  sequence, across both restarts and failed writes. An append failure
  ends the run: losing a committed event silently is worse than
  quitting. One process per log file - nothing enforces this. The file
  is one endless conversation; there's no session or conversation
  boundary yet.
- 2026-08-31: `seq` is removed from `Event`. Nothing read it but the
  counter that minted it, and a file's line order already carries the
  same ordering. A per-`App` counter also cannot survive the several
  writers the log is heading for. `Event` gains `source`: the writer
  that produced it. It is a plain `String` because that set is open,
  where `Actor` stays closed. Lines written before the field load as
  `unknown`. A message from any source is still `message.received` -
  `source` says where it came from, so a second type would only
  duplicate one payload shape. No reader filters by `source` yet. The
  TUI loads the log once at startup, so events another writer appends
  mid-session show up only on the next run.
- 2026-08-31: several writers may share one log. Every open, append,
  and load holds the file's advisory lock, superseding "one process per
  log file" above. The repair that trims a torn tail runs only under
  that lock, where a tail with no newline can only be a dead writer's.
  Without it a second process cannot tell that from a line still being
  written: it would cut away those bytes, the live writer's next chunk
  would land after the cut, and the fused remainder would fail every
  later load. The lock binds only processes that take it - a stray
  shell append is still unprotected - and it is unreliable on network
  filesystems.
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
