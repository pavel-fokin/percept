# AI agent instructions

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only.

## Architecture

Layer by dependency direction - each layer depends only on the one below
it, never sideways or up:

- **Domain**: entities and the capabilities they need, expressed as
  interfaces. No outward dependencies.
- **Application**: orchestrates domain objects to fulfill one use case.
  No vocabulary beyond the domain's.
- **Presentation**: renders and forwards input. No business logic.
- **Infrastructure**: implements the domain's interfaces - a database, an
  API client, a stub.

Wire concrete types together only at the entrypoint.

## Domain

- Distinguish entities (identity, tracked over time) from value objects
  (no identity, just data).
- The domain can own interfaces for capabilities it fundamentally needs,
  not just persistence - it should know the capability, never the
  mechanism behind it.

## ADR

- 2026-08-29: entity IDs use UUIDv7, each wrapped in a type specific to that entity, not a bare or shared ID type.

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
