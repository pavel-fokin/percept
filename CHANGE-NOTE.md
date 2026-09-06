# Shared maps: changes and rationale

This note describes `codex/shared-maps`, based on commit `e71bdb3`.
It is intended for comparing this implementation with the parallel solution.

## Problem

The workflow required a separate map entry for every settled choice.
That made accumulation inevitable. The rendered map also separated questions,
decisions, options, and relationships. Understanding one choice required
reconstructing it across several sections.

Continuous regrouping would reduce clutter at the cost of familiar references.
This implementation therefore changes both the admission rule and the view.
It retains the event log as the source of cognitive history.

## The three budgets

| Budget | Change | Reasoning |
|---|---|---|
| Overview | Introduce commitment entries with expandable supporting material. | Readers can start with a few lasting choices and inspect details selectively. |
| Change | Preserve existing node identities; group through explicit relationships. | A better overview should not erase familiar references or require relearning every choice. |
| Verification | Include source IDs, fragment coverage, and cross-boundary relationship counts. | Readers need evidence and a visible indication of what their selection excludes. |

These budgets guide agent behavior. The implementation does not impose numerical
limits or automatically measure human comprehension.

## Commitments and the overview

A commitment is a lasting choice whose forgotten rationale could cause a
mistake, repeated debate, or violation of a constraint. Routine settings and
task progress can remain in the experience log.

The decisions schema gains a `commitment` node kind and a `details` edge kind.
A details edge points from a commitment to supporting material. The renderer
uses these explicit relationships; it does not infer semantic groups.

The example [decision map](.percept/decisions.md) has four overview entries:

- Event storage
- Command suggestions
- Fireworks integration
- Stable shared maps

All 25 original nodes and seven original relationships remain. Four commitments
and 25 grouping relationships were added. The graph is larger; its overview
is smaller. This deliberately favors stable references over deleting detail.

Expandable sections keep rationale, supporting choices, and relationships together.
Ungrouped material remains available. Node IDs provide stable anchors, and
cross-links retain their source citations. Recorded text is escaped in HTML.

The three historical groupings are marked `proposed`. The shared-map policy
cites the user's request and authorization. Grouping approval does not imply
agreement with every interpretation of the historical choices.

Implementation: [schema and headlines](src/percept/map.rs),
[Markdown rendering](src/store/render.rs).

## Finding and checking information

| Question | Implemented approach |
|---|---|
| Which maps exist, and when should they be used? | A small [directory](.percept/index.md) describes their uses. `maps list` also reports purpose, origin, and size. |
| How is a relevant fragment selected? | The agent chooses an anchor and kinds. `read_map` and the CLI traverse explicit relationships to the requested depth. |
| When should evidence be checked? | Shared guidance calls for checking cited events before consequential corrections or resolving contradictions. A bare approval requires inspecting the proposal it approved. |
| What triggers revision? | Changed meaning, a new exception, a changed constraint, or a revealed misunderstanding. Another event alone is insufficient. |

The default `PERCEPT_MAPS` setting changes from `prompt` to `tool`.
The model receives map purposes and sizes, then requests relevant content.
Explicit `prompt` and `headlines` modes remain. `read_map` is available in
all modes so the model can retrieve source IDs.

Fragment responses report shown and total counts, crossing relationships, and
whether material was omitted. CLI JSONL remains on stdout; notices use stderr.
Markdown exports include the notice inside the document.

A complete recorded map can still contain incomplete or mistaken interpretations.
Boundary counts do not establish whether omitted material matters. Relevance
and evidence evaluation remain the agent's responsibility.

Implementation: [map selection](src/store/map.rs), [model tool](src/store/read_map.rs),
[CLI](src/cli/mod.rs), [prompt construction](src/app/mod.rs).

## Shared interpretation and revision

Human and agent understanding remain distinct. Shared maps expose interpretations
for discussion and correction; they do not prove shared understanding.

The [shared skill](.agents/skills/percept/SKILL.md), [workflow](AGENTS.md),
and reflection prompt apply the same admission and stability principles.
They favor local evidence and relationship changes over renaming or removal.
A revision should explain changed meaning, its evidence, and affected conclusions.
No revision is a valid result.

Agreement status uses existing string properties. It is an authoring convention,
not a compiler-enforced approval mechanism. There is no new in-place node
update operation. Removing a node still removes its incident relationships.
The guidance makes that consequence explicit before replacement.

## Claude Code and Codex setup

Both clients now discover the same skills in `.agents/skills`.
Claude's skill directories are symlinks. Developer instructions also have one
shared body, with small client-specific adapters. Shared review skills replace
dependence on client-specific review commands.

Versioned client configurations invoke one [capture script](scripts/agent-hook.py).
It records prompts, completed tool calls and results, and final replies.
It returns prompt event IDs for citation and preserves distinct client sources.
Causation state is isolated by client, checkout, session, and available turn ID.

Capture failures report an error without blocking coding. Missing binaries disable
capture. Oversized payloads remain subject to the publish CLI's argument limit.
Tests exercise hook contracts directly; they do not establish capture of every
internal client event. Codex hooks require trust and a new or restarted session.
Legacy Claude hook entries must be removed to prevent duplicate recording.

See [setup instructions](README.md) for commands and configuration locations.

## Isolation and validation

The comparison lives in `/private/tmp/percept-shared-maps` with an isolated
`PERCEPT_HOME`. Historical map events and their evidence were copied there.
Session statements were explicitly imported before being cited.

The generated Markdown is committed. The local event log and hook state are
not committed or pushed. A fresh clone therefore has the example view but
does not automatically acquire its live history or grouping commits.

Validation completed:

- Build, Clippy with warnings denied, formatting, and Git whitespace checks.
- 336 Rust tests and 17 hook tests.
- CLI checks for preserved records, source resolution, unique anchors,
  deterministic rendering, JSONL boundaries, and Markdown export notices.
- Local correctness and simplification reviews. Review fixed missing coverage
  notices in Markdown exports and incorrect origin text for code-map exports.

The remaining tradeoff is additional retrieval work for the agent in exchange
for less always-present context. The existing per-turn tool-call limit remains.
The human cost of these views still needs evaluation through actual use.
