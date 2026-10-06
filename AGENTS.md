# AI agent instructions

## Workflow

Non-trivial work runs plan, build, review. A one-line fix
skips it.

- **Plan.** The main agent breaks the request into issues via the `plan`
  skill. An issue has one clear outcome. It is product (a vertical slice
  of behaviour) or tech (refactoring, docs, tooling). Scope each as
  small as it goes. Decisions the user lives with - paths, filenames,
  flags, defaults - are settled with them before the build, never
  assumed. Where a function or a rule sits inside the code is not one
  of those: the builder proposes it, and review challenges it. The user
  agrees the set before any code; an explicit instruction to implement
  a proposal already discussed supplies that agreement. A candidate
  worth doing that nobody has committed to is never an approved issue:
  it is built only after the user has discussed it and agreed it into
  the set, and an agent that finds one while building says so.
- **Build.** An issue with no design left in it, touching one or two
  files, the main agent builds itself. Anything larger goes to the
  `software-developer` subagent, which follows this file, writes the
  code, runs the build and tests, and reports back. It does not design,
  choose scope, commit, or push.
- **Review.** The main agent checks each diff against its issue, and
  small fixes land there; larger rework goes back to the subagent.
  Then two passes run once each over the whole branch, before the user
  merges: one for correctness, defects a reader of the diff would not
  see, and one for simplification, complexity the diff adds that a
  simpler form removes. Two passes looking for different things catch
  more than a pass per issue. A branch that adds no branches, no
  I/O, and no behaviour change - a vocabulary or type addition, a
  rename, a doc edit - skips both. The main agent does one inline
  review pass instead. The two passes are for diffs with logic in them.

## Architecture

The crate follows domain-driven design in five layers. Each is one
module under `src/`, entered through `src/<layer>/mod.rs`, whose doc
comment states its role. The layout stays flat until a layer outgrows
one level.

A layer imports only the layers to its right:

- `cli` → `app` → `core` → `shared`
- `eventstore` → `core` → `shared`

`core` defines the store trait and `eventstore` implements it.
`main.rs` wires `eventstore` into `app`.

I/O is async on tokio. A trait method that does I/O returns
`impl Future<Output = …> + Send`, and async code never makes a blocking call.

Names carry no layer suffix or prefix: `Event`, not `EventEntity`.

## Code Quality

Entity IDs use UUIDv7, each wrapped in a type specific to that entity,
not a bare or shared ID type.

Comments earn their place. Prefer a clear name to a comment. Never
restate what the code does - comment only a complex algorithm,
non-obvious business logic, or a "why" the code can't show.

Keep it simple. Don't make a thing optional when the compiler can
enforce it. A boolean defaults to false, never to optional. Trust
Rust's type system rather than writing defensive checks around it.

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only. One commit per issue.

Work happens on a branch. Check which one is checked out before the
first commit - a status snapshot from the start of a session can be
stale - and switch to main before branching, never onto another feature
branch, so a PR carries only its own commits. Name a branch
`<type>/<branch-name>`, `type` one of `feat`, `fix`, `chore`, `docs`, `refactor`,
matching the issue's own kind. Merging into main is the user's call,
not the agent's - hand back a reviewed branch and stop there. The same
holds for pushing: before it, propose a PR title for the user to
confirm or change.

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
