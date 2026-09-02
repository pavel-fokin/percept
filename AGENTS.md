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
| Domain | `percept` | `Event`, `Message`, `Model` - entities and the capabilities they need, as interfaces. Serde-free; depends on `shared` and on `futures-core`, for the stream type its reply port returns. |
| Application | `app` | `App` - orchestrates domain objects for one use case, no vocabulary beyond `percept`'s. |
| Presentation | `tui` | Renders the transcript, forwards input. No chat logic of its own. |
| Presentation | `cli` | `percept events publish`, `search`, `show` - the log without the TUI. |
| Infrastructure | `providers` | `Ollama` today, more LLM clients later - implements `percept::Model`. |
| Infrastructure | `store` | The JSONL event log - the serde boundary - implements `percept::EventLog` and `EventSearch`. |
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
- 2026-08-31: `Event` gains `Payload::ToolUsed { body: String }` for a
  tool call from another writer. A `Payload` variant is typed only when
  the domain reads it: `to_messages` needs `content`, so
  `MessageReceived` stays typed; nothing in the domain reads a tool
  call, so `body` is the raw JSON text its source sent, unparsed.
  `to_messages` now filters to `message.received` - a tool call is no
  longer fabricated into dialogue. On the wire, `payload` is a real
  nested object rather than an escaped string, so a caller can still
  index into it with `jq`. A known type carried opaquely is not the
  same as accepting arbitrary types - `store::decode` still rejects a
  `type` it doesn't know. Superseded 2026-09-02: `ToolUsed` folds into
  `ToolCalled`, so a foreign tool call carries the same typed shape
  percept's own loop emits.
- 2026-09-01: searching the log is a capability of its own, separate
  from persisting it. `percept::EventQuery` names a query and
  `percept::EventSearch` answers one; `store::Jsonl` implements both
  ports. `EventQuery` also decides what matches, so each rule lives in
  the type whose doc comment states it and a store only supplies the
  events. Before this the CLI held the filtering, comparing domain
  values in the presentation layer. `EventQuery` carries absolute
  timestamps and never reads a clock - a relative shorthand like `1d`
  resolves in the CLI, before the domain sees it. `since` is inclusive
  and `until` exclusive, so adjacent windows tile with no overlap and
  no gap. A multi-valued filter matches any of its values; an empty one
  is off. `size` replaces `limit` - the N most recent matches, still
  printed oldest-first. There is no index: `search` loads and then
  filters. Filtering before decode would silently skip a line naming an
  unknown event type, and failing loudly is worth more than the speed.
  Full-text search is not in yet. On the command line, `percept events
  search` replaces `list`.
- 2026-09-01: `EventKind` is the domain's word for what the wire calls
  `type`, with one variant per `Payload` variant; `store` owns the
  spelling. Rust reserves `type`, and `Kind` is what the ecosystem
  names a discriminant, so the two layers differ by a word on purpose.
  Fetching one event by its id is `EventLog::get`, not a search -
  identity is not a criterion to match on - and `show` uses it rather
  than loading the whole log to scan it. `EventLog::load` stays because
  the TUI rebuilds the transcript at startup. It can go once that read
  is a search with an empty query, leaving `EventLog` as `append` and
  `get`.
- 2026-09-01: `percept::Model` returns a stream of chunks instead of
  taking an `on_chunk` callback. A trait method can't return `impl
  Stream`, so the port is a boxed, pinned `ReplyStream`. `reply` no
  longer returns a `Result`: a connection that never opens is the
  stream's first `Err` item, so one error path covers a failure before
  a reply and during it. The domain names `futures_core::Stream` for
  this, its first dependency beyond `shared` - `tokio_stream` would
  have pointed the domain at a runtime. `Chunk` separates a model's
  `Thought` from its `Reply`. Reasoning split from the answer is the
  shape every current provider streams, not an ollama detail.
- 2026-09-01: replies come from a local ollama server.
  `providers::Ollama` posts to `/api/chat` and reads its NDJSON body,
  one JSON object per token. Chunk boundaries don't align with lines,
  so the reader buffers an incomplete tail. The URL and model are
  `const` in `main` - `http://localhost:11434` and `gemma4` - not flags
  or environment variables, so there is one place to change them.
  `reqwest` carries no TLS feature: localhost is plain HTTP, and a
  hosted provider can add one later. Only the connect is bounded, at
  five seconds. A first token can be minutes away while ollama loads a
  model, so a read timeout would abort healthy replies. `Stub` is
  deleted rather than kept behind a flag, so `scripts/drive.py` now
  needs a live server.
- 2026-09-01: a thinking model's reasoning is a fact the log records.
  `Payload::ThoughtRecorded` is its own variant, wire type
  `thought.recorded`, sharing `message.received`'s payload shape. A
  turn commits up to two events - the thought, then the reply, both
  caused by the prompt - superseding "one `Event` is committed when the
  reply completes" above. `to_messages` filters a thought out; it is
  never replayed to the model as dialogue. The TUI shows it dimmed
  while it streams and never again, so a reloaded transcript is
  dialogue only. This amends the typed-variant rule: a variant is typed
  when the domain produces or reads it. `App` assembles a thought from
  streamed text.
- 2026-09-01: a failed reply is shown, never logged. The provider's own
  words reach a transient `error` on the TUI's `Chat`, cleared at the
  next submit. Sending them as a chunk would commit them as something
  the model said, and an append-only log keeps that forever. Whatever
  text did arrive still commits. `App` refuses a second `submit` while
  a turn streams: without the guard the first reply took the second
  prompt's `causation_id` and both replies fused into one event. The
  guard lives in `App`, which owns the turn, not in the TUI that types
  into it.
- 2026-09-01: the TUI reads as a chat, not a log dump. A turn is a
  marker in a two-column gutter - `>` for the user, `⏺` for the model,
  `✻` for a streaming thought - with the body hanging under itself and
  a blank line between turns. The `You: ` prefix is gone: a prefix
  inside the text moves as the text wraps, where a gutter stays a
  column. Input sits in a rounded box with its own `>`, so it reads as
  somewhere to type rather than the transcript's last line. A status
  row under it shows one of three things: the error in red, wrapped
  and capped at four rows; a spinner with `Thinking…` until the first
  reply chunk and `Responding…` after, because a first token can be
  minutes away; or `Enter send · Esc quit`. The spinner ticks from the
  main loop, and only while a turn streams - idle, nothing moves, so
  no frame is worth redrawing. The error moves out of the transcript
  into that row, where the rest of the not-logged text already lived.
- 2026-09-02: the model can call one tool, `search_events`, and the
  turn loops until it stops calling. `percept::Tool` is a domain port
  with a `ToolSpec` (name, description, a JSON-Schema string - the
  domain stays serde-free); `store::SearchEvents` implements it,
  turning JSON arguments into an `EventQuery` and returning `summarize`
  lines. It lives in `store`, not a `tools` module, because it needs
  `store`'s wire helpers and a sibling infra module would be a
  sideways dependency. A turn commits `tool.called` (actor `Model`)
  then `tool.resulted` (actor `System` - its first producer), the
  result caused by the call, the next model events caused by the
  result; `Message` grew `Text`/`ToolCall`/`ToolResult` variants so
  the pair replays. `App` owns the loop: `begin_tool` records the call
  and returns a `ToolStep` (run this tool, carry on, or stop), the
  presentation layer only carries the step out; `finish_tool` commits
  the result and re-asks. It is capped at five calls a turn - past the
  cap the request carries no tools and `begin_tool` stops the turn.
  Tools run on a blocking thread, off the single-threaded runtime, so
  a full-log scan never freezes the UI. `search_events` returns the
  newest 20 matches unless asked for more. `Model::reply` now takes a
  `ModelRequest` (messages plus tool specs), and `App` prepends a
  transient system message with the current time so the model resolves
  relative dates itself. The TUI shows both tool events dimmed under a
  `⚒` gutter and keeps them on reload - this reverses "a reloaded
  transcript is dialogue only" for tool activity, though a recorded
  thought still never replays.
- 2026-09-02: `Payload::ToolUsed` is removed. It predates `ToolCalled`
  and carried a foreign writer's tool call as an opaque JSON blob - the
  domain never read it, `to_messages` filtered it out, and the TUI hid
  it. Now one variant, `ToolCalled`, serves both: percept's own loop
  and another writer like claude-code. Foreign writers adopt percept's
  typed `{tool, arguments}` shape rather than their own, because it is
  percept's log. A foreign `ToolCalled` replays to the model as
  `Message::ToolCall` even though no `ToolResulted` follows in this
  log - it is context, better shown than hidden. Old `tool.used` lines
  stop loading; the log is throwaway during development, so no alias
  or migration. The typed-variant rule stands: a variant is typed when
  the domain produces or reads it, and `ToolCalled` already was.
- 2026-09-02: `EventQuery` gains a text filter, closing the gap the
  2026-09-01 search ADR left open. A term matches when one of the
  event's payload strings - `content`, `tool`, `arguments` - carries it
  as a case-insensitive substring; several terms match any-of, the
  grammar every other multi-valued filter uses. The envelope is not
  searched: `actor` and `source` already have filters, and matching
  them under a second one would blur which filter owns what. A blank
  term is contained by everything, so each boundary that can receive
  one - the CLI flag, the tool's arguments - rejects it before a query
  is built. The CLI flag is `--contains`, naming the relation where the
  other flags name fields, because no field called `text` exists to
  name; the domain field is `text`, and `search_events` exposes the same
  filter as `contains` - the model's only lever for finding an event by
  what it says, where a shell user has `grep`. Still no index: a
  substring scan over a log `search` already loads whole.
- 2026-09-02: the model reads a window of the log, not all of it -
  `CONTEXT_EVENTS`, the newest 20 events, applied in
  `App::build_request`. Until now every event went into the prompt, so
  `search_events` could only return what the model had already read and
  no run could tell searching from reading. The window is what makes
  that claim testable, and it is the primitive's premise: looking is
  cheap, so a model looks rather than holds. `App::events` keeps the
  whole log - the TUI renders all of it, the same split that already
  lets a thought be shown and never replayed. The cut can fall between
  a `tool.called` and its `tool.resulted`, so a window opening on a
  result drops it: `Message::ToolResult` is `role: "tool"` on the wire,
  and no provider accepts a conversation starting there. The number is
  a `const`, not a flag, following `OLLAMA_MODEL` - one place to change
  it. Windowing by event count rather than characters is deliberate: a
  fact can be planted at a known depth. The model is not told its
  history is truncated; whether saying so changes how often it searches
  is the next thing to measure, held back so this window's effect can
  be read on its own.
- 2026-09-02: `percept ask "<prompt>"` runs one turn headlessly - the
  same `AppService` policy the TUI drives, without a terminal. It exists
  to make a turn repeatable: a chat UI is a poor instrument for running
  one prompt many times. It stamps `source` `cli`, so a run is
  recoverable with `percept events search --source cli`. The reply goes
  to stdout and every tool call and result to stderr, so stdout pipes
  clean while the loop stays watchable. `ask` accumulates the reply
  itself rather than reading `pending_reply` at the end: `App` clears
  that buffer at each tool call and again when the cap ends a turn, so
  text spoken before a call would never be printed. No channel and no
  spawned task - `tui`'s mpsc plumbing exists to keep the UI drawing,
  and nothing else needs the thread here, so a tool runs inline. The
  turn policy is not duplicated; it already lives in `App::begin_tool`
  and `finish_tool`, and both presentations only carry the step out.

## Workflow

Non-trivial work runs plan, build, review, reflect. A one-line fix
skips it.

- **Plan.** The main agent breaks the request into issues via the `plan`
  skill. An issue has one clear outcome. It is product (a vertical slice
  of behaviour) or tech (refactoring, docs, tooling). Scope each as
  small as it goes. Decisions the user lives with - paths, filenames,
  flags, defaults - are settled with them before the build, never
  assumed. Where a function or a rule sits inside the code is not one
  of those: the builder proposes it, and review challenges it. The user
  agrees the set before any code.
- **Build.** An issue with no design left in it, touching one or two
  files, the main agent builds itself. Anything larger goes to the
  `software-developer` subagent, which follows this file, writes the
  code, runs the build and tests, and reports back. It does not design,
  choose scope, commit, or push.
- **Review.** The main agent checks each diff against its issue, and
  small fixes land there; larger rework goes back to the subagent.
  `/code-review` and `/simplify` then run once each over the whole
  branch, before the user merges. Two passes looking for different
  things catch more than a pass per issue. A branch that adds no
  branches, no I/O, and no behaviour change - a vocabulary or type
  addition, a rename, a doc edit - skips both. The main agent does one
  inline review pass instead. The skill passes are for diffs with
  logic in them.
- **Reflect.** Close the session by proposing changes to this workflow,
  but only when a step strained or missed something. A session where
  the process fit the work needs no reflection. Cutting a step counts
  for more than adding one. Aim for the smallest process that still
  catches mistakes.

The TUI only runs on a real terminal. `scripts/drive.py` forks a pty,
sends timed keystrokes, and prints the frames; `--plain` strips the
escapes so the rendered text can be grepped.

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only. One commit per issue.

Work happens on a branch. Merging it into main is the user's call, not
the agent's - hand back a reviewed branch and stop there. The same holds
for pushing.

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
