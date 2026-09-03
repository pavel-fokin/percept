# AI agent instructions

## Purpose

`percept` is an experimental harness that aims to implement
the following cognitive arhitecure.

An agent has an immutable history of experience and a mutable set of explicit
external representations constructed from that experience.
Different representations make different reasoning operations cheap.

Experience Log
       │
       │ search
       ▼
      LLM
       │
       │ constructs / revises
       ▼
┌────────────────────────────┐
│ External Representations   │
│                            │
│ taxonomy                   │
│ glossary                   │
│ decision model             │
│ task model                 │
│ causal model               │
│ ...                        │
└────────────────────────────┘
       │
       │ cognitive commits
       ▼
Cognitive History

The shape comes from Recursive Language Models (arxiv.org/abs/2512.24601),
where a model holds a corpus as an environment and writes programs over it
instead of reading it as prompt text.

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
| Infrastructure | `providers` | `Ollama` and `OpenAi` - implement `percept::Model`. `PERCEPT_PROVIDER` picks one at the entrypoint; `OPENAI_API_KEY` carries the key. |
| Infrastructure | `store` | The JSONL event log - the serde boundary - implements `percept::EventLog` and `EventSearch`. |
| Foundation | `shared` | `Id<T>`, `Timestamp` - value types with no domain meaning. Below the domain; depends only on `uuid`, `jiff`. |

Wire concrete types together only at the entrypoint - `main` in Rust.

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

## Code Quality

Entity IDs use UUIDv7, each wrapped in a type specific to that entity,
not a bare or shared ID type.

Comments earn their place. Prefer a clear name to a comment. Never
restate what the code does - comment only a complex algorithm,
non-obvious business logic, or a "why" the code can't show.

Keep it simple. Don't make a thing optional when the compiler can
enforce it. A boolean defaults to false, never to optional. Trust
Rust's type system rather than writing defensive checks around it.

Don't pad errors. Skip `.context("Failed to X")` when the error
already says it failed. Clean up stray logs as you find them; add a
log only for an error or a security event.

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

Target Flesch-Kincaid grade 12 or below. Treat it as a smoke test,
not a gate -- professional terms inflate the score honestly. If
writing scores above grade 12, look at sentence length and clause
nesting first, never at vocabulary. Simplifying words instead of
sentences produces vague prose with a good score, which is the
failure this rule exists to prevent.
