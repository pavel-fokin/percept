# AI agent instructions

## Purpose

`percept` is an experimental harness on cognitive rails. The rails
constrain what an agent may write to its record and read from it.
They prescribe no thinking loop and filter nothing the model says. On
them an agent keeps an immutable history of experience and a mutable
set of maps built from it. A map is an explicit external representation - a
decision map, a task map, a glossary - and each kind makes a different
reasoning operation cheap. The shape comes from Recursive Language
Models (arxiv.org/abs/2512.24601), where a model holds a corpus as an
environment and writes programs over it instead of reading it as
prompt text.

    Experience Log
           │
           │ search
           ▼
          LLM
           │
           │ constructs / revises
           ▼
    ┌────────────────────┐
    │ Maps               │
    │   decisions        │
    │   tasks            │
    │   glossary         │
    │   taxonomy         │
    │   domain           │
    │   ...              │
    └────────────────────┘
           │
           │ commits
           ▼
    Cognitive History

The experience log is percept's event log. It never ranks, summarises,
or answers: the model judges relevance, and percept's job is to make
looking cheap, so output is constant-size per event by default. A map
is where the model's own summaries live. Every change to a map is a
cognitive commit - one event in the same log, citing the experience it
was derived from. A map is folded from those commits, so the cognitive
history rebuilds it deterministically; the experience alone does not,
because a second pass through the model gives a different map. The
model builds maps today; the user co-owns them. Human and agent each
keep their own understanding. A shared map is where the two are
compared and corrected, and a recorded claim is not the human's
agreement.

## Domain

Two modules. `core` holds experience and the maps folded from it -
`Event`, `Source`, `Actor`, `Map`, `Schema`, `Selection`, `Usage`, and
the ports to append, load, search, and render. `harness` holds what a
loop needs to drive a model over `core` - `Message`, `Model`, `Tool`,
`Snapshot`, and the tool-approval `Policy` - and depends on `core`.
Both are serde-free.

- `Event` is an append-only log entry: `id`, an `actor`, a `source`, an
  optional `causation_id`, a `created_at`, and a typed `payload`. Once
  committed it never changes. `file.cited` is the payload for a
  file, or a range of it, as it was seen at that moment; a node that
  lists its id in `sources` cites that text, and the session-start
  block reports the node when the text is no longer in the tree. On
  the wire a line also carries a `LogCursor`: the id of the log that
  wrote it, minted once into the `log-id` file beside the log, and its
  `seq` in that log, so two logs can be merged later without a
  migration. A line written before that reads as this log with `seq`
  equal to its line number. The store writes and reads the cursor;
  the domain does not carry it yet.
- `Source` names the writer that produced an event - `percept-code`,
  `percept-cli`, `claude-code`, `codex` - and the project root it ran in. The
  name is open by design, where `Actor` is closed. One log at
  `$PERCEPT_HOME/percept.jsonl`, `~/.percept` by default, holds every
  project; the path is how a fold picks one project out of it.
- `Message` is a value object (no identity) - the shape `Model` needs to
  talk to an LLM. Derived from the log at the boundary, never stored.
- `Actor` (`Human`, `Agent`, `System`) is the one vocabulary for who a
  message or event is attributed to; a human carries an id once a
  server has registered them, kept in the `me` file beside the log, so
  a change says by whom. Until then they have
  none, and the log they wrote is who they are.
- `Model` is domain-owned, not infrastructure: `percept` needs "a reply
  given messages," never the mechanism behind it.
- `Map` is a cognitive map: nodes and edges the model builds from the
  log, folded from `node.added`, `node.changed`, `node.removed`,
  `edge.added`, and `edge.removed` events in the same log. A `Schema` names a map and
  the node and edge kinds it allows, and one line of purpose - what
  the map makes cheap - that the prompt carries in place of the map
  itself. A schema is a TOML file at `.percept/schemas/<name>.toml`,
  so a session adds a map without a Rust change; `decisions` and
  `tasks` ship built in as the same TOML, and a project file of the
  same name extends one - it keeps every kind and headline the
  built-in declares and may add kinds - never shrinks it, since the
  log and the render already rest on those kinds. A kind may list the
  properties a node must carry - `why` on an option or a task - and
  the values its `state` may hold, a set with no value open by
  position; the write path refuses a node without them. Every change
  goes through `Map::apply`, so the rules live once. The core is
  specified in `docs/architecture.md`: five events, four invariants,
  six write rules, and no kind name.
- A node records who added it and when, and who last changed it and
  why - `changed_by`, `changed_why`. One rank rule says who may change
  what: a human is above an agent; renaming or removing another's node
  needs rank; `state` is anyone's; and a change from above locks the
  node against everyone below, so after the user's why on an agent's
  decision the agent can only add beside it. A user-written node is
  the human's landmark in a shared map: the model may attach edges to
  it and set its state, nothing else. A decision is corrected by
  adding the new one with a `supersedes` edge to the old, never by
  rewording, so the old landmark stays one hop away. A model that finds a decision no longer fits does not
  supersede it: it raises a question with a `reopens` edge to the
  decision, which stands until the user settles the question.
  Stability of the representation is a value beside accuracy
  and compactness: a map may grow, but what a reader has seen does not
  move. The core keeps no standing and no read receipt: the user's
  correction is the node's last change, printed wherever the node is.
- `Scope` says which project's events a fold reads: the current one by
  default, every one with `--all-projects`. A map is read live, never
  rendered to a file a session commits: one log holds every branch, so
  a committed render would carry whichever branch's fold wrote it
  last. `percept maps show <map> --format md` from the shell,
  `read_map` mid-turn, or the bounded fragment a session-start hook
  prints give the same Markdown a render once did. A render lists a
  map's headline nodes in the order they were raised, each with its
  properties, its last change, and its edges by name; a node of
  another kind is one hop away, under the edge that reaches it, or
  with `percept maps show decisions --around question:<name>`.
  `--since <time>` on `maps show` lists what a map gained since a
  reader last looked. A `Selection` - around a node, since an instant,
  of some kinds - cuts a map to a `Fragment`, which counts what the cut
  left out and how many edges cross it. `maps show` and `read_map` both
  cut through it, so the order of the cuts lives once.

## Maps

Start with [.percept/index.md](.percept/index.md): one row per map
saying what it is for, where it comes from, and how to open a fragment
of it. The shared [percept skill](.agents/skills/percept/SKILL.md)
covers selecting a fragment, checking a claim, and revising.

A map is judged by what it costs its reader, on three budgets:

| Budget | What to keep small |
|---|---|
| Overview | Concepts held at once to understand one decision. |
| Change | New meaning to absorb after an update, and landmarks moved. |
| Verification | Work to check a conclusion and see what a correction touches. |

The change budget weighs most. Add beside what a reader has seen;
never move or merge it without the user's say.

## Decisions

The decisions map for this repo, folded live from percept's own log:
`percept maps show decisions --format md`, or the bounded fragment a
session start prints. Every node cites the event it was drawn from. It
is the record of why; where it disagrees with a rule above, the rule
wins and the map says what the rule cost.

## Architecture

Layer by dependency direction - each layer depends only on the one below
it, never sideways or up:

| Layer | Package | Owns |
|---|---|---|
| Domain | `core` | `Event`, `Source`, `Actor`, `Map`, `Schema`, `Schemas`, `Selection`, `Usage` - experience and the maps folded from it, with the rules between them, plus the ports to append, load, search, and render. Serde-free; depends on `shared`. |
| Domain | `harness` | `Message`, `Model`, `Tool`, `Snapshot`, `Policy` - what a loop needs to drive a model over `core`. `Policy` says whether a tool call runs at once or asks the user; `Snapshot` saves the working tree under a prompt and puts it back. Depends on `core` and on `futures-core`, for the stream type its reply port returns. |
| Application | `app` | `App` - orchestrates `core` and `harness` for one use case, no vocabulary beyond theirs. Runs the tool loop: commits `tool.called`, asks the `Policy`, hands the caller a `ToolStep` - run, ask the user, or carry on. A `Harness` groups what `App` is given: the tools, the policy, the cap, the snapshot, the instructions, and a `Context` - the list of sections the request carries, stable first for the provider's cache, with history sized to a share of the model's window and the events just past it indexed one line each. `MapShape` says how much of each map the prompt carries; `PERCEPT_MAPS` sets it at the entrypoint. `docs/harness.md` is the design. The `code` toolset - the TUI's default, `PERCEPT_TOOLS=code` elsewhere - adds the file tools, the policy that asks before a write, a cap of fifty calls, a snapshot per prompt, and the checkout's `AGENTS.md` as system text every round; `undo` restores the last one. |
| Presentation | `tui` | Renders the transcript, forwards input. No chat logic of its own. A `ToolStep::Ask` pauses the turn on a row: `y` runs once, `a` runs and allows that tool for the session, `n` declines; `/undo` puts the tree back. |
| Presentation | `cli` | `percept events publish`, `search`, `show`, `percept maps` - the log and its maps without the TUI; `ask` and `reflect` under `lab`. `hook <client>` records a coding client's turn from the hook JSON on stdin; `init <client>` writes the client's config to call it. Headless, a call the policy would ask about is declined unless `ask --yes`. |
| Presentation | `server` | `percept review` - serves the embedded review page, the queue as `GET /api/review` JSON, and `POST /api/change` - Wrong, a `node.changed` carrying the human's why - over the same log and maps the CLI uses. The queue is what changed since the review last opened. |
| Infrastructure | `providers` | `Ollama`, `OpenAi`, and `Fireworks` - implement `harness::Model`. `PERCEPT_PROVIDER` picks one at the entrypoint; `OPENAI_API_KEY` and `FIREWORKS_API_KEY` carry the keys. |
| Infrastructure | `store` | The JSONL event log - the serde boundary - implements `core::EventLog` and `core::EventSearch`. `event` encodes an event to a log line and back, and reads one out for display. |
| Infrastructure | `mapstore` | Loads the schemas - the built-in TOML plus `.percept/schemas/*.toml` - and folds a log-backed cognitive map (`LogMaps`, the `core::MapReader`), revises it, and gives it an external form: `encode_*` to JSON lines, `markdown`/`catalogue` to the text `maps show`/`maps list --format md` print - read live, never written to a file. |
| Infrastructure | `code` | Walks the working tree with `ignore`, parses each file with `tree-sitter`, and builds a `Map` of the tree's files, the symbols they define, and what imports what: a `file` keyed by repo-relative path, a `function` or `type` keyed by `path::Name`, a `package` per external crate. Not a map percept keeps - it has no author and no history, is never folded from the log, never in the catalogue, never carried in the prompt. It reaches the model only as the `read_code` tool. |
| Infrastructure | `tools` | Every tool the model calls. `search_events`, `read_event`, `revise_map`, `read_map` run over the log and its maps through `store` and `mapstore`. `read_code` walks the checkout through `code` with the same `around`, `depth`, and `kinds` as `read_map`. `read_file`, `write_file`, `edit_file`, `list_files`, `find_files`, `grep_files` run over a working tree, native over the `workspace` module's `Workspace` - the one place a path the model gave becomes a real path, refusing any outside the checkout, shared with the CLI's citations - and `bash`, one `sh -c` at the root with a timeout. The file tools and `read_code` come in under `PERCEPT_TOOLS=code`. `AskBeforeWrites` is the `Policy`; `GitSnapshot` the `Snapshot`, a commit under `refs/percept/snapshots/<prompt>` built through a scratch index. |
| Infrastructure | `workspace` | `Workspace`, `is_binary`, `read_text_lossy` - the one place a path the model gave becomes a real path, and the text reader both a file tool and the CLI's `file.cited` citations read through. Builds without `lab`. |
| Foundation | `shared` | `Id<T>`, `Timestamp` - value types with no domain meaning. Below the domain; depends only on `uuid`, `jiff`. |

`app`, `harness`, `tui`, `providers`, `code`, and `tools` build only
under the `lab` Cargo feature, off by default; `workspace` stays, for
the CLI's citations. Test and lint both builds: with `--all-features`
and without.

Wire concrete types together only at the entrypoint - `main` in Rust.

## Coding clients

percept serves whichever coding client the user runs - Claude Code,
Codex - so the setup in this repo is client-neutral. (percept's own
coding agent, `percept-code`, is a fourth surface, not a client: it
reaches the log through the harness, not the CLI.) Instructions live in
`AGENTS.md`, skills in `.agents/skills`, the subagent body in
`.agents/agents`, and event capture in the binary as `percept hook
<client>`, which records under the client's name as the source. A
client's own folder - `.claude`, `.codex` - holds only discovery
metadata and the commands that call the shared files: a symlink, a
settings file that `percept init <client>` writes, an adapter. A new
client costs an adapter, never a copy. A rule only one client can follow is not a rule of this
repo.

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
  agrees the set before any code; an explicit instruction to implement
  a proposal already discussed supplies that agreement. Each settled
  decision is then recorded in the decisions map as `model`, citing
  the prompt that settled it, so the next session does not reopen it.
  An option is recorded only for an alternative that lost, with the
  reason it lost; the pick is the decision itself. A decision that
  changes an earlier one is added with a `supersedes` edge to it; the
  old node is never removed. An idea in the ideas map is a candidate,
  never an approved issue: it is built only after the user has
  discussed it and agreed it into the set, and an agent that finds one
  while building leaves it there and says so.
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
  more than a pass per issue. Each client runs them with what it has;
  `CLAUDE.md` names Claude Code's. A branch that adds no branches, no
  I/O, and no behaviour change - a vocabulary or type addition, a
  rename, a doc edit - skips both. The main agent does one inline
  review pass instead. The two passes are for diffs with logic in them.
- **Reflect.** Close the session by proposing changes to this workflow,
  but only when a step strained or missed something. A session where
  the process fit the work needs no reflection. Cutting a step counts
  for more than adding one. Aim for the smallest process that still
  catches mistakes. An approach the session tried and abandoned goes
  into the decisions map as evidence, so no later session tries it
  again. Work the session found and left undone goes into the tasks
  map with its why, and an issue that was an open task is closed
  there with the commit in its change's why, so the next session starts from the list and not
  from a re-read.

The TUI builds under `--features lab` and only runs on a real
terminal. `scripts/drive.py` forks a pty,
sends timed keystrokes, and prints the frames; `--plain` strips the
escapes so the rendered text can be grepped.

A command the agent runs is killed after ten minutes, in the background
too. A script that runs longer - an experiment under `experiment/` -
is launched detached with `nohup`, writing to a log, and watched
through that log.

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

## Testing

Tests live in the crate, beside the code they cover. `percept` is a
binary with no library target, so a top-level `tests/` directory would
see nothing internal. A test reaches private items by nesting under the
module it tests, as `mod tests { use super::* }`.

Every `mod tests` sits in its own file, never inline. The
implementation file keeps `#[cfg(test)] mod tests;`; the cases move to
a sibling `tests.rs` nested under that module - `src/app/mod.rs` beside
`src/app/tests.rs`, `src/core/map.rs` beside
`src/core/map/tests.rs`. A file and its same-named directory
coexist, so the implementation file keeps its name and needs no
`#[path]`. Split an inline module the next time you touch its tests,
not before.

Shared fakes live in `src/core/testing.rs` and `src/harness/testing.rs`,
each beside the port it fakes. Each implements one port and nothing
more, so it sits at the domain's level and every layer above can use it
without bending the dependency direction.

One behaviour per test. The name states the behaviour, not the method -
`streamed_reply_commits_one_event_caused_by_the_prompt`.

## Git

Use conventional commit messages under 72 chars. Skip the body -- subject
line only. One commit per issue.

Work happens on a branch. Check which one is checked out before the
first commit - a status snapshot from the start of a session can be
stale - and switch to main before branching, never onto another feature
branch, so a PR carries only its own commits. Merging into main is the
user's call, not the agent's - hand back a reviewed branch and stop
there. The same holds for pushing.

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
