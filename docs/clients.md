# percept in a coding client

A proposal, written 2026-09-08. It reimagines how percept serves a
coding client such as Claude Code or Codex. `docs/architecture.md`
names the core and its surfaces; this document designs the first
surface in that table, skills and hooks for a client. Nothing here
changes behaviour on its own. Standing, confirm, and finish below
were replaced on 2026-09-11 by the node's last change and the rank
lock; the core in `docs/architecture.md` is current.

## The claim

A client already has memory. percept adds evidence, co-ownership,
and one log. The flow below is built around the moments where those
three pay, and leaves everything else to the client.

Three things follow:

- The map leaves the prompt. A session starts from a bounded
  fragment computed at that moment, not from a render committed in
  the tree.
- The human gets a surface of their own. Hook output reaches the
  model, never the screen, so the human's side of a shared map needs
  a page they open. It shows a cut sized to a sitting and asks one
  thing: is anything here wrong.
- Reflect and review close into one loop. The model opens questions
  when a fact changed; the human answers them; the next session
  starts from the answers.

## The session

| Moment | percept | Who pays |
|---|---|---|
| Start | A session-start hook prints a bounded fragment into the model's context: what the maps gained since this person's last session here, open questions, open tasks, and decisions whose cited files changed. | A hook. Nothing committed. |
| Look | The model opens a fragment when a design question comes up. | The model, one call. |
| Agree | The user says yes to a proposal. That prompt is the source. A hook on that prompt pushes the open questions and the decisions citing the files the plan names. The model records the decision, and each alternative that lost with its reason. | The model, once per settled question. |
| Fail | An approach tried and abandoned goes in as evidence. | The model, when it backs out. |
| Reflect | The last thing the model does in the session. Over the fragment it touched, it asks whether each decision still holds against what changed. It writes evidence and opens a question only when a fact changed. | The model, in-session, where the context is already loaded. |
| End | A stop hook lists the claims the session made. The model's final message repeats the count, since that is what the human sees. | Nothing new. |
| Review | The human opens the review page, from the branch or the PR, marks what is wrong, and finishes. What was in the cut and not marked is seen. | One keystroke per wrong claim. Finishing is one event for the cut. |

The next session, in either client, starts from the same log and sees
the review's answers. That is the one-log claim at its cheapest point.

### What it looks like

The start block, the whole map presence in the prompt:

```
percept · project percept · maps: decisions, tasks
since your last session here (2026-09-07 18:40)
  decisions +3   tasks +1   confirmed by you: 2   commented: 1
  d41 decision "a requires list on the kind; option and task require why"
  d42 decision "TOML: name, purpose, headlines, settles, [[nodes]], [[edges]]"
open questions
  q17 "How does the prompt stay within the model's window as the maps grow?"
  q18 "How does a rendered map stay scoped to the branch it is committed in?"
changed since decided
  d38 cites src/mapstore/schema.rs, changed 2026-09-08
fragment: percept maps show decisions --around q17
```

The review page, per since-cut:

```
Four claims to decisions since you last reviewed, on 7 September at 18:40.

q21  How does a schema name the edges that move a node between states?
     Raised on 8 September at 10:02. Settled by d43.

 ◌   decision, a [[lifecycle]] table on the schema ...          Wrong   confirm
d43  because the fold derives the state; the renderer stops branching on kind names
asks The model asks you to look: it changes what the render shows for every past decision.
you  Claimed by the model. Not seen by you.
     ▸ You said "approve" at 10:14. Show the exchange.

 ●   weighed and lost, a settles flag on each edge kind          Wrong   confirm
o12  because one flag cannot say which state the edge leaves and which it enters
     Disputed by you.

     evidence, state derived in the renderer: tried, moved to the fold
e07  Evidence has no standing.

                                                      [Finish review, one seen]
```

### What a client needs

| Client | Files | Content |
|---|---|---|
| Claude Code | `.claude/settings.json` | Hook lines for session start, prompt, and stop, each calling the binary with the client's name, and tool use under `--capture`; a bash allowlist for `percept maps`, `percept events`, and `percept start`. `percept init claude-code` writes them. |
| Codex | `.codex/hooks.json` | The same, in its hook shape, without an allowlist. `percept init codex` writes it. |
| Both | nothing | The session-start block is what `percept start` prints: what each map holds, what moved since the last session and which cited files changed, and the commands to go next. `percept maps describe <map>` is where the record grammar lives. |

The plan skill goes. The trigger is "the user said yes to a proposal,"
which is a prompt, and every client hooks prompts. A client's plan
mode is one way that happens, not the rule.

### A worked example

Two days, two clients, one change: lifecycle on schemas.

**Day one, Claude Code.** The user opens `claude` in the checkout.
The session-start hook runs the binary and prints the start block
above into the model's context. The user types:

```
Add lifecycle to schemas: resolves settles a question, supersedes makes a decision past.
```

The prompt hook records the prompt and returns its event id, `p71`,
into context. The model enters the client's plan mode. Before
proposing, it looks at the question nearest the change:

```
percept maps show decisions --around q19
```

It sees the TOML decision, that it was superseded once, and why. It
proposes a `[[lifecycle]]` table in the schema file and names the
alternative it rejected. The user replies `approve`. That prompt,
`p72`, is the source. The hook on it pushes the open questions and
the decisions citing `src/mapstore/schema.rs` back into context, and
the model records against them, one document on stdin:

```
percept maps record decisions --actor model --source p72 <<'EOF'
question "How does a schema name the edges that move a node between states?"
decision "a [[lifecycle]] table on the schema: edge kind, from state, to state"
  why "the fold derives the state; the renderer stops branching on kind names"
  resolves question
option "a settles flag on each edge kind"
  why "one flag cannot say which state the edge leaves and which it enters"
  answers question
EOF
```

The reply names what it wrote: `q21 d43 o12`. The build runs as the
client runs it. Midway the model tries deriving the state in the
renderer, backs out, and records that:

```
percept maps record decisions --actor model --source p72 <<'EOF'
evidence "state derived in the renderer: tried, moved to the fold"
  about q21
EOF
```

As its last act the model reflects over the fragment it touched. The
start block said `d38` cites a file that changed today. It reads the
diff, finds the decision still holds, and writes nothing. The session
ends. The stop hook lists the claims, and the model's final message
says so where the human can see it:

```
Recorded to decisions: q21, d43, o12, e07. Review with percept review.
```

**The review.** Before opening the PR, the user runs `percept review`.
The page opens on four claims since their last review. `d43` carries
the model's mark that it changes a render rule, so they read it and
its exchange, and leave it. They read `o12` and mark it wrong: nobody
proposed a flag, the alternative weighed was two edge kinds. They
press `f`. The cut closes, `d43` is seen, and the block at the foot
shows the two lines the model will get. Two keystrokes and one
sentence; the dispute was recorded when they saved its why.

**Day two, Codex.** The user opens Codex in the same checkout. The same
hook, pointed at the same binary, prints the start block. It now
carries the review:

```
since your last session here (2026-09-08 11:24)
  reviewed by user: d43 seen, o12 disputed
  o12 disputed: "Nobody proposed a flag. The alternative I weighed was two edge kinds ..."
```

The user asks for the render to show a past decision differently.
The model opens the fragment around `q21`, sees the confirmed table
and the disputed option, and builds on the table. Seen is enough:
the human read it with the model's mark on it and did not object. It does not propose
a flag. It records the disputed option correctly this time, with a
`supersedes` edge to `o12`, so the wrong one stays one hop away. That
is the check from the architecture doc: a decision not reopened, and
a correction that cost one keystroke and one sentence.

## The changes

Each row is one issue. Order is by what the others rest on.

| # | Change | Why |
|---|---|---|
| 1 | The hook moves into the binary as `percept hook <client>`, the event read from the input. `percept init <client>` writes the config lines and the allowlist. Built 2026-09-08. | One installed binary and config pointing at it. No Python on the path. Without the allowlist, recording costs three permission prompts. |
| 2 | A fold budget on every hook call, measured; a fold cache if the budget fails on a large log. | The log is one file for every project and grows forever. A slow start hook gets disabled, and the whole flow rests on it. |
| 3 | A short id on every node, printed in every output and accepted by every verb. A record verb that takes a document on stdin. | A node's name is its identity, so an edge needs fifty characters reproduced inside shell quoting. The model gets it wrong. Typed arguments are the fix; MCP is one transport for them, not the only one. |
| 4 | The start hook prints the bounded fragment above. The decisions render leaves `AGENTS.md` and is no longer committed. | Kills the map budget problem and the unmerged-branch render at once. The prompt carries a purpose line per map and a capped cut, as the harness already does. |
| 5 | A push hook at the yes moment: open questions and the decisions that cite the files the plan names. | Told once is not a habit. The model records against what it was just shown, not what it remembered to look up. Matching a name is a fact; percept may do it. |
| 6 | `disputes` and `confirms` edges from the human, a `review.finished` event naming the cut, and standing derived in the fold: claimed, seen, confirmed, disputed. An `asks` mark the agent sets on a claim it wants read. A comment is a user-written note attached by an edge. | Seen is the batch, derived from the cut and costing nothing per claim. Wrong is the one action. A note is the human's landmark, so the model may not remove it. |
| 7 | `percept review`: a local page served by the binary, a cut sized to a sitting, one action per claim and one to finish. Reachable from the branch and, as a projection, from a PR comment listing the claims. The page is designed below. | A static render cannot write. A web app waits for a second person. The human's best moment is reading the diff, so the page meets them there. |
| 8 | A node cites a file through a `file.cited` event - the text as it was seen - listed in its sources. The start hook prints `changed since recorded`: current headline nodes whose cited text is no longer in the tree. Built 2026-09-09 as decisions d76 to d83. | Gives reflect a fact to start from, so it does not reread the whole map. |
| 9 | Reflect runs in-session as the last thing the model does, over the fragment it touched. It writes evidence and opens a question only when a fact changed. It never confirms or disputes. | Only a human confirms. The model's reflect feeds the review queue; a stingy reflect keeps that queue readable. |

## The surface

Two cognitions share the map, and each is bounded. The log is the only
unbounded thing. The bounds differ in mechanics and scale, and the
surface between the two is where they meet.

| Limit | Agent | Human |
|---|---|---|
| Capacity | The window. Hard, in tokens, known per model. | Working memory. Soft, a handful of items, unknown per person. |
| Degradation | Attention thins as the window fills. The middle is read worst. | Attention thins as the queue grows. Past ten, everything is waved through. |
| Rot | Within a session. What it loaded at the start is stale after its own edits, and it does not notice. | Across sessions. What they knew on Monday is gone by Thursday, and they know it. |
| Time scale | Minutes, one turn. | Days, one project. |
| Remedy | Re-cut, do not accumulate. A fresh fragment at the moment it matters. | Re-enter through what changed since they last looked. |

Three things follow.

**One mechanism, two sizings.** Both remedies are a cut of the same
map: around a node, since an instant, of some kinds. The harness sizes
the agent's cut to a share of the window; the review page sizes the
human's to a sitting. One `Selection`, two budgets. A context failure
on either side is fixed in the cut, never by a map.

**Rot is fixed by when the cut is taken, not how big it is.** The push
hook at the yes moment exists because the model's start-of-session
view is stale by the time it decides. Reflect at session end and the
"changed since decided" line are the same remedy. For the human, the
since-cut on entry is the remedy at the other time scale. A bigger cut
makes rot worse on both sides, since more of it is old.

**What crosses is a cut the human chose, never a ranking the system
did.** percept does not rank, and this is where the rule matters most.
Three things set the cut, none of them the system's judgement:

| What sets the cut | Who sets it |
|---|---|
| The kinds that cross: decisions and tasks, never evidence | The schema, its headlines |
| Since when, and around what | The human, once, as a default |
| "This one needs you", a mark on a claim | The agent, as a claim like any other |

The last row is the agent's only way to raise a claim above the cut.
It is inside the log, so it can be wrong and can be disputed. An agent
that marks everything is one the human stops reading, and the tally
shows it.

**The human writes one thing.** Standing has four values and the human
writes only the last: claimed, the agent wrote it and nobody has
looked; seen, it was in a cut the human opened and finished; confirmed,
the human said so, kept for the few claims worth it; disputed, with a
why. Seen is derived from the `review.finished` event and the cut it
names, so it costs nothing per claim. A claim both cuts have contained,
the agent's when it wrote and the human's when they finished, is the
only shared understanding the system can vouch for. A claim in neither
cut is in the log, findable, and nobody's.

**Review is the surface's operation, and its cost is the constraint.**
Everything else in this document sits on one side or the other. The
start block, the push hook, reflect, and the record verb are the
agent's and can be as busy as they like. The review page is the
surface, and it must be readable in one sitting or it does not exist.

## The review page

`docs/review-sketch-mvp.html` is the sketch the MVP builds: the
Changes view for decisions and tasks, mobile first, with Wrong, a
quiet Confirm, and Finish behind one check. `docs/review-sketch.html`
is the earlier, fuller sketch; the Map view, comments, undo, and the
asks mark below wait for it. Together they settle the following.

**Two views per map, one for each budget the maps are judged by.**
Changes is the since-cut as a queue, grouped by the map's headline
kind, and is the same for every map. Map is the whole map in the
shape its schema declares. The source fold and a neighbourhood around
one node serve the verification budget in both.

| Shape | Fits | Reads as |
|---|---|---|
| stream | decisions | Questions in the order raised, each with the decision that settles it now, the superseded one folded under it |
| board | tasks | Columns by state, a blocks edge written under the task it holds up |
| tree | taxonomy, glossary | An indented outline, a glossary as the flat case |
| graph | domain, architecture, flow | Nodes and edges drawn, entered through a fragment |

The shape is declared on the schema beside purpose and headlines, so
a session that adds a map says how it is read. A drawn graph of more
than about forty nodes is unreadable, so a graph map's overview is
its headlines and the reader zooms into a fragment around one node.
The since-cut highlights what moved in that picture, which is the
one thing a queue cannot show: where a change sits.

**Actions are identical in every view, and the views link both
ways.** Confirm, dispute, and comment sit on a node whether it is a
queue row, a board card, or a graph node. A claim in the queue opens
its neighbourhood in the map; a node in the map shows its standing and
its recent claims. One renderer, two outputs, so the human and the
model read the same lines.

**The page is a register.** A margin column carries the standing as
a drawn mark and the node's short id: a dashed ring for claimed, a
solid ring for seen, a filled check for confirmed, a filled bar for
disputed. A claim the agent marked carries "asks you" under its id. The record
itself is set in a serif; controls and ids in a sans. Kinds are
lead-in words, `decision,` `weighed and lost,` `evidence,` and meta
is sentences. Evidence carries no mark: it has no standing.

**Recognition over recall.** The source of a decision is usually
"yes". The source line shows what the user said and when, and folds
open to the proposal the yes answered. The source stays the prompt;
the display walks one event back.

**Each save is one event, and undo is a retraction.** Finishing
records one event for the cut. Wrong opens a why box and records only
on save,
since a dispute without a why is the node the write path refuses. A
note is a user-written landmark attached to the node. Every save
shows an undo for a few seconds; undo appends a retraction, never
deletes, and the toast says "Retracted".

**Wrong is the one action.** Every row has one button, Wrong, which
opens the why box. Confirm is a quiet secondary for the claim worth
saying more about, and comment shows on hover, focus, or the focused
row. The header carries "Finish review, 3 seen": one click closes the
cut, and what was not marked becomes seen. Under ten claims per
sitting reads; thirty gets finished unread, so reflect must be stingy
with what it adds to the queue.

**The human's words visibly reach the model.** A block at the foot of
the queue shows the next session's start lines with the review's
answers in them, so a dispute or a note is seen to land.

**One key per action.** `j` `k` move, `w` marks wrong, `f` finishes,
`y` confirms, `c` comments, `s` opens the source, `m` flips between
the views. A bar at the bottom names them and `?` hides it.

## Costs and limits

- **The review page is a queue, and a queue has a length.** Under ten
  claims per sitting reads. Thirty gets finished unread, and seen then
  means nothing. The page is for spotting the wrong claim, not reading
  every one: headline and why visible, evidence folded, the agent's
  mark on the ones it wants read.
- **The page must show the exchange, not the prompt.** The source of
  a decision is usually "yes." The page walks one event back and shows
  the proposal the yes answered. The source stays the prompt.
- **The human's words must visibly reach the model.** A comment comes
  back in the next start block. The page says so, or the human writes
  one and never learns whether it landed.
- **The start block is capped.** After a two-week gap the since-cut
  is fifty lines. Counts and a pointer replace the rest.
- **One renderer, two outputs.** The human reads the page and the
  model reads the fragment. If they differ, the two contours argue
  about different maps.
- **The human loses the map in the diff.** With the render no longer
  committed, someone who read decisions in a PR reads them on the
  review page instead. The PR comment in change 7 is the replacement.

## Decisions that are yours

Two. Everything else in the changes table is the builder's, in the
order given, and review challenges where it lands.

1. **The map leaves the prompt.** The start hook prints a cut; the
   render is no longer committed. Change 4.
2. **Review is wrong-only, and finishing is the batch.** Seen is
   derived from the cut. Changes 6 and 7.

## Decisions this reopens

- **"Flag decisions whose subject left the code map as stale"** lost
  once on node count. Change 8 and 9 are the same idea with a
  different writer: percept counts, and the model writes evidence only
  when it found something. If agreed, it supersedes that rejection.
- **"How does a rendered map stay scoped to the branch it is committed
  in?"** and **"How does the prompt stay within the model's window as
  the maps grow?"** are both open. Change 4 settles them by not
  committing the render and not carrying the map.
- **"Confirmation must cost a keystroke or a batch."** The
  architecture doc's rule. Seen keeps the batch and drops the
  keystroke: finishing the cut is the batch, and it is one event.
  Confirmed stays as the explicit mark for the few claims worth it.
- **MCP.** Distribution is the same: a stdio server is a subcommand of
  the same binary. What it changes is typed arguments in the model's
  tool list. Change 3 gets most of that through the CLI. Revisit MCP
  only if the session tally shows the model still not looking.

## Validation

The check stays the one in `docs/architecture.md`: per session, count
decisions reopened, plan steps re-derived, and claims the human
corrected. Two things make it cheaper here. The stop list gives the
session's claims to tally against. The review page records the
corrections as events, so the third count is a fold, not a memory.
A fourth count, decisions recorded with no file.cited event in their
sources, says whether citing has become a habit; if it stays high, the
fix is a `cites` line on the record verb, not a rule in the skill.

## Recommendation

Build changes 1 to 4 first. They cost nothing the flow does not need
and remove two open problems. Run the tally for a few sessions on that
alone. Then 5 and 6, which make the Agree moment reliable and give
confirmation an operation. The review page and reflect come last, once
the queue they feed has claims in it worth reviewing.
