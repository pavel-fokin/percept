# The MVP

A product hypothesis, written 2026-09-09. It names who percept is
for, the job it takes on, what the MVP must show, how it is used, and
what is left to build. `docs/architecture.md` names the core;
`docs/clients.md` designs the client surface. This document sits
above both and says what they are for.

## The job

One strong developer runs coding agents all day, across sessions and
across clients. A new job came with the agents: keep a coherent model
of what they changed and learned.

Today it is done by hand.

- Read the diff.
- Recall earlier conversations.
- Check the ADRs.
- Re-explain the context at the start of each session.
- Correct the same misconception again.
- Notice an architectural consequence after the fact.

No tool owns this job. An agent's memory serves the agent. Git serves
the code, not the model of it. An ADR serves the human, but the agent
does not write one and the human stops after the third.

percept keeps project decisions alive across AI coding sessions. That
is the sentence a stranger reads first, and the README opens with it.
percept automates the coordination, not the coding; the rest of this
document says what keeping a decision alive takes.

## The hypothesis

The job is done through one representation that the human and the
agents both write to and both can check. It pays only when three
properties hold.

| Property | The human gets | The mechanism |
|---|---|---|
| Visibility | What the agent added to our model of the system since I last looked. Durable semantic changes, never the transcript. | The since-cut of a map's headline kinds, sized to one sitting. |
| Correctability | "This part is wrong," said once, and it outlives the session. | A `node.changed` from the human carrying only a why, kept on the node as its last change, and a rank lock that stops the agent rewriting it. |
| Continuity | The next agent does not start from zero. It inherits the model with its evidence, its standing, the corrections, and the disagreements. | The session-start cut carries each gained node's last change and the human's words, in any client. |

This is what "keep humans in cognitive control" means. Not approval
of every action. The ability to see, correct, and keep what was
understood. The rails in `docs/architecture.md` are the mechanism;
control is the promise.

Confidence is not a fourth property. A number the agent attaches to
its own claim is a ranking, and percept does not rank. Evidence and
the node's last change say the same thing without the number.

## What proves it wrong

Any of these, over a run of real sessions on the developer's own
project.

- The developer stops opening the review after a few sessions. The
  job exists, but not at this price.
- The review is finished unread. Visibility is not consumed: the cut
  is too big, or it says nothing they did not know.
- A correction does not change agent behaviour. A claim the human
  wrote on comes back, or a settled decision is reopened anyway. Continuity failed.
- The client's own memory alone shows the same drop in reopenings.
  Then percept delivered memory, not control, and the three properties
  added ceremony.

## The constraint that decides it

The review is a queue, and a queue has a length. Under ten claims per
sitting gets read. Thirty gets finished unread, and then seen means
nothing.

This repo is the evidence. Four days of building percept with percept
put 84 questions in its decisions map, most of them written by the
model, under a workflow that records at every settled step of a plan.
That is twenty a day. Nobody reads twenty a day, and the person who
built it did not.

The fix is never a rule asking the model to write less. Two things set
the queue, and both are the MVP's to get right.

| What sets the queue | How |
|---|---|
| When the model records | At the moment the user says yes to a proposal, pushed by a hook, not at every step of a plan. One record per settled question. |
| What the review shows | Headline kinds only: decisions and tasks, never options or evidence. Those stay one hop away. A question that reopens a decision is raised above the rest. |

Open as q84 in the decisions map.

## Scenarios

Four sittings, one developer, two clients.

**First session.** They install percept, run `percept init
claude-code` in an existing project, and open Claude Code. The
session-start hook prints an empty fragment and three rules: look
before proposing, record when the user says yes, cite the prompt. They
ask for a change. The model proposes, they say yes, and the hook on
that prompt pushes the open questions into context. The model records
the decision and the alternative that lost, citing the yes. The
session ends with one line they can see:

```
Recorded to decisions: q1, d1, o1. Review with percept review.
```

Nothing else changed. No file in the tree, no prompt they wrote.

**The review.** Before opening the PR they run `percept review`. A
page opens on the claims since their last review, grouped by
question. Each row is a headline and a why, with the exchange behind
the yes one click away. One claim is wrong: the alternative the model
says lost was never proposed. They press `w`, write one sentence, and
press `f`. The page shows the two lines the next session will start
with. Two keystrokes and one sentence.

**The next session, in Codex.** The same binary prints the start
block. It carries the review: the claim the human wrote on, with their
sentence. The model builds on the seen decision and does not propose
the disputed alternative. It records the correction with a
`supersedes` edge, so the wrong claim stays one hop away and leaves
the headlines.

**A month later.** A decision cited a range of a file, and the file
changed. The start block says so. The model reads the diff as its last
act of the session, finds the decision no longer holds, and opens a
question with a `reopens` edge to the decision. It does not rewrite
the decision: that is the human's to do. The review shows the question
above the rest. The human answers it in a sentence, and the next
session starts from the answer.

## What is built and what remains

| # | Change | State | Serves |
|---|---|---|---|
| 1 | Hooks in the binary, `percept init` writes the config | Built 2026-09-08 | Install |
| 2 | Short ids on every node, `maps record` on stdin | Built 2026-09-09 | Recording |
| 3 | The start block prints a since-cut; the render leaves the tree | Built 2026-09-09 | Visibility, continuity |
| 4 | A node cites file text; the start block reports what changed | Built 2026-09-09 | Evidence |
| 5 | The start block carries the recording rules | Built 2026-09-10, removed 2026-09-12: the rules were a loop prescription; `percept start` and `maps describe` replace them | Recording in a stranger's project |
| 6 | A push hook at the yes moment | To build | Recording as a habit; the queue |
| 7 | A `reopens` edge in the decisions schema, from a question to the decision it challenges | Built 2026-09-10 | An agent disputes without rewriting; the review raises it |
| 8 | A node's last change, `changed_by` and `changed_why`, and the rank lock; Wrong is a `node.changed` with a why, `maps change-node` from the shell | Built 2026-09-10 as standing, rebuilt 2026-09-11 | Correctability |
| 9 | `percept review`: the page, wrong-only; the cut is since the review last opened, no Finish | Built 2026-09-10, Finish removed 2026-09-11 | Visibility, correctability |
| 10 | The start block carries who last changed each gained node and why | Built 2026-09-10, rebuilt 2026-09-11 | Continuity |
| 11 | A tally per session | To build | Falsifiability |
| 12 | The hook records prompts and replies; tool capture only behind `init --capture` | Built 2026-09-10 | Trust; the fold stays small |
| 13 | Release binaries and a curl install | To build | A stranger installs |
| 14 | `init` writes the local, uncommitted config | To build | A teammate without percept is unharmed |
| 15 | Actors are `human`, `agent`, `system`; a human carries an id once a server registers them | Built 2026-09-10 | Who confirmed; the wire an object before a stranger has a log |
| 16 | Every event carries its log's id and seq; a `LogCursor` names a position | Built 2026-09-10 | Merging logs later, without a migration |

Built so far: 5, 7, 8, 9, 10, 12, 15, 16. The order from here is 11,
then two weeks of the tally on this repo and one other, then 13 and
14, which a stranger needs and the hypothesis does not. 9 is the
mobile-first page in `docs/review-sketch-mvp.html`: the queue, Wrong
with a why, a quiet Confirm, Finish behind one check, and the
exchange behind each claim; the Map view, comments, and undo wait. 15
and 16 change the wire, so they land before 13 while the only log is
the author's.

## The tally

After two weeks, three questions the developer answers about
themselves.

- Did I stop repeating settled decisions to agents?
- Did I notice a decision no longer fit the project before it cost
  something?
- Did my correction reach the next session, in the other client?

A no on any of them is the hypothesis failing. The counts below, per
session and by hand until change 11 lands, say which property failed.

| Count | Says |
|---|---|
| Decisions reopened | Continuity failed. |
| Misconceptions repeated | A correction did not land. |
| Claims corrected, and what each cost | Correctability's price. |
| Review opened, and claims read before finish | Visibility's price. |
| Claims in the queue per sitting | Whether the constraint above holds. |

## Out of scope

- `percept-code`, the TUI, `percept ask`, and `percept reflect`. The
  coding agent is a lab for one claim, the log as environment, and not
  part of what a developer installs.
- The `plan` skill. Its trigger, the user said yes, is a prompt, and
  every client hooks prompts. The push hook replaces it.
- SDKs and the crate split. Both wait for a second consumer.
- Tool capture. Four days of this repo put 74 MB in the log, and 73
  of them are tool calls and their results. No MVP mechanism reads
  them: the since-cut and the start block run on prompts,
  replies, and map events, and `file.cited` names the text a claim
  rests on. What they cost is a stranger's secrets under `~/.percept`
  and a fold that reads everything to find a little. Capture stays
  behind an `init` flag for the `percept-code` lab, and the fold
  budget, q71, leaves the MVP with it.
- A percept server. The JSONL file under `~/.percept` is the MVP's
  substrate, one person on one machine. A server later syncs the log
  across machines and a team under the same rules, and that is where
  co-ownership becomes multi-party and a human contour needs a name,
  open as q61.

## Against the field

Agent memory layers give the agent memory: Zep, Mem0, Letta, Cognee
extract facts and rank them on retrieval. Repo-native memory for
coding agents keeps distilled knowledge in git: Kage verifies a
memory's citations at write time and withholds stale ones, Mainline
seals an intent record to each commit, AsDecided lets only humans
write and agents only read. Memco pools abstracted patterns across
teams.

None of them lets the human say "wrong" on an agent's claim so that it
outlives the session, and none keeps the human's correction on the
node the agent wrote. That is the ground the hypothesis stands on, and it
is the part not built.
