# Seeding the decisions map from AGENTS.md

One sitting that writes the decisions AGENTS.md already embodies into
the decisions map, so `.percept/decisions.md` is not empty when the
measurement in `README.md` starts. Run it once, in this repo, with the
branch `feat/claude-code-writer` merged and `~/.percept/bin/percept`
installed by `scripts/install.sh`.

Two choices to confirm before starting, both cheap to change later:

- The set below is eight decisions. Add or drop before recording, not
  after: a node once written stays in the log.
- The source event carries the rule's full text as AGENTS.md has it.
  A one-line summary would be shorter and less honest.

## What qualifies

The map's schema is question, option, evidence, decision. A rule with
no alternative named is a rule, not a decision, and waits for the
`rules` map described in `../map-kinds.md`. Seed only the decisions
where AGENTS.md or `../maps/README.md` names what lost.

| # | Question | Decision | Rejected option, as the text gives it | Source section |
|---|---|---|---|---|
| 1 | Where do tests live? | beside the code, `mod tests` in a sibling `tests.rs` | a top-level `tests/` directory, which sees nothing internal in a binary crate | AGENTS.md, Testing |
| 2 | Who owns `Model`? | the domain | infrastructure | AGENTS.md, Domain |
| 3 | How is the `code` map built? | walked from the working tree, never in the prompt | folded from the log like the other maps | AGENTS.md, Domain and Architecture |
| 4 | What identifies an entity? | UUIDv7 in a type per entity | a bare or shared id type | AGENTS.md, Code Quality |
| 5 | How much of a map reaches the prompt? | the whole map, `PERCEPT_MAPS=prompt` | headlines or tool, which paid a call each and gained little | maps/README.md, one map many questions |
| 6 | Can a map node be uncited? | no, `revise_map` refuses it | allowed, as the first reflect runs produced | maps/README.md, results 2026-09-03 |
| 7 | Who merges and pushes? | the user | the agent | AGENTS.md, Git |
| 8 | Which review passes run, and when? | code review and simplify once per branch, skipped for a branch with no logic | one pass per issue | AGENTS.md, Workflow, Review |

## Procedure

Every command below uses `P=~/.percept/bin/percept`. Run from the repo
root, so the project path on every event is this repo's.

### 1. Read the map as it stands

```
cat .percept/decisions.md
```

A node whose name already exists is refused, so anything already there
is skipped below. The log-location decision from the build session is
already in.

### 2. For each row: publish the source, then record

The decisions predate the log, so each needs an event to cite. Publish
the rule's text as one `message.received` from actor `user`, since the
user wrote AGENTS.md. The content opens with where the text comes from
and the commit, so a reader of the event knows it is a quotation, not a
prompt. The `id` the publish prints is the source for every node of
that decision.

Row 1, in full, as the pattern:

```
P=~/.percept/bin/percept
commit=$(git rev-parse --short HEAD)

id=$($P events publish --actor user --source claude-code --type message.received \
  --payload "$(jq -cn --arg c "AGENTS.md at $commit, Testing: Tests live in the crate, beside the code they cover. percept is a binary with no library target, so a top-level tests/ directory would see nothing internal. A test reaches private items by nesting under the module it tests, as mod tests { use super::* }. Every mod tests sits in its own file, never inline." '{content:$c}')")

$P maps add-node decisions --kind question --name "Where do tests live?" --source $id
$P maps add-node decisions --kind option --name "a top-level tests/ directory" --source $id
$P maps add-node decisions --kind option --name "beside the code, mod tests in a sibling tests.rs" --source $id
$P maps add-node decisions --kind evidence --name "a top-level tests/ sees nothing internal in a binary crate" --source $id
$P maps add-edge decisions --kind contradicts \
  --from "evidence:a top-level tests/ sees nothing internal in a binary crate" \
  --to "option:a top-level tests/ directory" --source $id
$P maps add-node decisions --kind decision --name "beside the code, mod tests in a sibling tests.rs" \
  --prop why="private items stay reachable through super::*; one file per test module keeps the implementation file readable" --source $id
$P maps add-edge decisions --kind resolves \
  --from "decision:beside the code, mod tests in a sibling tests.rs" \
  --to "question:Where do tests live?" --source $id
```

Rules for the other rows:

- The decision node is named as the option it picks, exactly.
- `why` is one line, in the words the source gives.
- An evidence node and its `contradicts` edge only where the source
  says why the loser lost. Rows 1, 3, 5, 6 have one; rows 2, 4, 7, 8
  do not, so skip the evidence lines there.
- Names are quoted in `--from` and `--to` as `kind:name`, and a name
  with a colon in it still splits on the first one, so keep colons out
  of names.

Source text per row, to paste into the publish:

| # | Source text |
|---|---|
| 2 | AGENTS.md, Domain: `Model` is domain-owned, not infrastructure: `percept` needs "a reply given messages," never the mechanism behind it. |
| 3 | AGENTS.md, Domain and Architecture: `code` is a `Map` too, but folded from the working tree instead of the log. The `code` map walks the working tree with `ignore`, parses each file with `tree-sitter`, and builds a `Map` of `file`, `function`, `type`, and `package` nodes - `maps list` and `maps show` read it, but it is never folded from the log and never reaches the model's prompt. |
| 4 | AGENTS.md, Code Quality: Entity IDs use UUIDv7, each wrapped in a type specific to that entity, not a bare or shared ID type. |
| 5 | experiment/maps/README.md, one map many questions: prompt 15/15 correct at 0.3 calls per question; headlines and tool 15/15 at 1.1 and 1.3 calls. Headlines and tool cost one call and gain little over prompt. The whole map is cheaper than the search that replaces it. |
| 6 | experiment/maps/README.md, results 2026-09-03: No sources were cited. Every reflect-built node has an empty sources list. A map without sources is a summary, and a summary cannot be checked. Fixed since: `revise_map` refuses an uncited node and the reflect prompt says to search first. |
| 7 | AGENTS.md, Git: Merging into main is the user's call, not the agent's - hand back a reviewed branch and stop there. The same holds for pushing. |
| 8 | AGENTS.md, Workflow, Review: `/code-review` and `/simplify` then run once each over the whole branch, before the user merges. Two passes looking for different things catch more than a pass per issue. A branch that adds no branches, no I/O, and no behaviour change skips both. |

### 3. Read the render as the next session will

```
cat .percept/decisions.md
```

Expect eight questions, eight decisions, sixteen options, four evidence
nodes, and twelve edges, plus the log-location decision already there.
Every node has a `sources` line. If one is missing, its publish step
was skipped: the map refuses uncited nodes, so this cannot happen
silently, but check.

### 4. Commit the render alone

```
git add .percept/decisions.md
git commit -m "chore: seed the decisions map from AGENTS.md"
```

Nothing else changes in the tree. The events live in `~/.percept`.

### 5. What is left out, on purpose

The Code Quality and Writing sections are conventions with no
alternative named: comments earn their place, booleans never optional,
one idea per sentence. They are the content of the `rules` map, not
this one. Recording them here as decisions would pad the render with
nodes that have no question.

## After seeding

The measurement in `README.md` can start. Its t1 and t3 tasks are
written against rows 3 and 1 of this table.
