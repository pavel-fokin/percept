# AI agent instructions

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only.

## Architecture

Layer by dependency direction - each layer depends only on the one below
it, never sideways or up:

| Layer | Package | Owns |
|---|---|---|
| Domain | `percept` | `Event`, `Message`, `Model` - entities and the capabilities they need, as interfaces. No outward dependencies. |
| Application | `app` | `Conversation` - orchestrates domain objects for one use case, no vocabulary beyond `percept`'s. |
| Presentation | `tui` | Renders the transcript, forwards input. No chat logic of its own. |
| Infrastructure | `providers` | `Stub` today, real LLM clients later - implements `percept.Model`. |

Wire concrete types together only at the entrypoint - `main` in Rust.

## Domain

- `Event` is an entity (has an ID, tracked over time); `Message` is a
  value object (no identity) - the shape `Model` needs to talk to an LLM.
- `Model` is domain-owned, not infrastructure: `percept` needs "a reply
  given messages," never the mechanism behind it.

## ADR

- 2026-08-29: entity IDs use UUIDv7, each wrapped in a type specific to that entity, not a bare or shared ID type.
- 2026-08-30: Rust is the implementation language, not Go. Both were built
  in parallel to compare the stack; Rust wins going forward. The Go
  implementation is removed - it isn't kept as a reference.

## Writing

Chats and docs are both text. Write for a specific reader.

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
