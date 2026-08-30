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
| Infrastructure | `codec` | JSON wire format for the event log - the serde boundary. Consumed once persistence lands. |
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
  format lives in `codec`, which rejects event types it doesn't know
  rather than the DTO failing to parse. `Id<T>` and `Timestamp` live in
  `shared`, below the domain. Streaming is separate from the log: reply
  chunks reach the UI transiently and one `Event` is committed when the
  reply completes.

## Workflow

Non-trivial work runs plan, build, review. A one-line fix skips it.

- **Plan.** The main agent breaks the request into issues via the `plan`
  skill. An issue has one clear outcome and at most five settled
  decisions. It is product (a vertical slice of behaviour) or tech
  (refactoring, docs, tooling). Scope each as small as it goes. The user
  agrees the set before any code.
- **Build.** Each issue goes to the `software-developer` subagent. It
  follows this file, writes the code, runs the build and tests, and
  reports back. It does not design, choose scope, commit, or push.
- **Review.** The main agent checks the diff against the issue, then runs
  `/code-review`. Small fixes land here; larger rework goes back to the
  subagent.

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
