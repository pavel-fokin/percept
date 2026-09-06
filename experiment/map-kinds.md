# Which cognitive maps are worth trying next

`decisions` is the one map folded from the log today. The Purpose
section of AGENTS.md lists others - tasks, glossary, taxonomy, domain -
as examples. This note ranks the candidates, with a schema sketch for
each, so a trial can start as an issue rather than a discussion.

## The test a map kind has to pass

A map earns its place when it makes one reasoning operation cheap. Four
questions decide whether a kind is worth building:

- **Operation.** Which question does it answer without a search?
- **Not in the tree.** Does the working tree or git already answer it?
  The code map lost to `rg` because the tree had cheap looking already.
- **Write moment.** When does the content exist in full, so a writer
  can record it without being asked?
- **Read moment.** When would a session read it, and is that moment
  the same as another map's? Maps read at the same moment belong in
  one schema, so the render stays one file. A different read moment
  earns a schema of its own.

## The candidates

| Map | Operation it makes cheap | Write moment | Read moment | What competes |
|---|---|---|---|---|
| experiments | what has been shown, what is still assumed, what the next run should test | after each experiment run | designing the next experiment | the README results tables, by hand, without sources |
| attempts | has this been tried, and how did it fail | a session abandons an approach, or a fix lands after a failure | on a failure, keyed by the error text | nothing; abandoned work never reaches git |
| tasks | what is open, blocked, or cut, and by what | plan creates, commit closes | session start | git log, for closed work only |
| sessions to files | which sessions touched this file, so "why is it like this" has a place to start | none; derived from `tool.called` arguments | before editing a file | git blame, which gives commits, not the reasoning around them |
| rules | which convention applies here and what decided it | a rule enters AGENTS.md | every session, as the doc itself | AGENTS.md by hand, with no provenance |
| glossary, domain, taxonomy | what a word means here, what is a kind of what | rare | rare | the Domain section of AGENTS.md, which already is one |

## Ranked

### 1. experiments

percept is a research project. `experiment/*/README.md` already holds
this map's content: questions, the conditions that test them, results,
and what each result showed. It is written by hand and cites nothing.
The operation, "what is still assumed", is the one every planning
conversation on this repo does from memory.

| Node kind | Example |
|---|---|
| `question` | does a map answer questions it was not built for |
| `hypothesis` | a map in the prompt replaces the search |
| `run` | `runs/20260903-113242-generalise`, with condition and model as properties |
| `result` | prompt: 15/15 correct, 0.3 calls per question |

Edges: `tests` from a run to a hypothesis, `supports` and `refutes`
from a result to a hypothesis, `raises` from a result to a new
question. A result cites the events the run's script publishes, once
`run.sh` publishes its summary rows as events.

Read moment: the plan step of any experiment issue, and the "what this
does not tell you" paragraph every README ends with.

### 2. attempts

The one map with a different read moment. Every other candidate is
read at session start. An attempt is read when something fails, and
the key is exact: the error text. That makes it the one map where the
tool condition should work. The model has a precise string to search
for and a reason to search at that moment. It also holds the most
expensive thing a fresh session does: repeat an approach that already
failed.

| Node kind | Example |
|---|---|
| `attempt` | fold the transcript per project |
| `outcome` | `failed` or `worked`, with the symptom text as a property |
| `cause` | the TUI window then showed another project's chat |

Edges: `ended_in` from an attempt to an outcome, `because` from an
outcome to a cause, `instead` from a failed attempt to the attempt that
replaced it. An attempt cites the tool event where it failed.

Write moment: the reflect step names abandoned approaches, and a fix
committed after a red test names the symptom it cleared.

### 3. sessions to files

Derived, like `code`: no schema to fill and no writer to teach. Fold
the `file_path` arguments of Edit, Write, and Read out of `tool.called`
events, group by the prompt chain that caused them, and the result is
a map from each file to the sessions that touched it. Through those
sessions' prompts it reaches the decisions recorded in them. It is the
bridge between the experience log and the code map that nothing else
provides, and it costs nothing to try once the hooks have recorded a
few sessions.

Node kinds: `file`, `session` (the root prompt's event id, with its
first line as a property). Edges: `touched`, with the tool name as a
property. Read from `maps show sessions --around file:<path>`.

### 4. tasks

Valuable, and half of it is in git already. The half that is not -
what was cut, what is blocked and by what, what the next session
should pick up - is small on a solo project. Try it after the two
above show whether session-start reads pay at all.

Node kinds: `task`, `blocker`. Edges: `blocks`, `part_of`, `resolved_by`
pointing at a decision or a commit. A task cites the plan prompt that
created it.

### 5. rules

The endgame, not a trial. Every rule in the Code Quality, Testing, and
Writing sections of AGENTS.md becomes a node citing the session that
introduced it, with an edge to the decision that motivated it. Then
AGENTS.md is a render, the user and the model co-own it, and "why do we
do this" has an answer with a source. Do it when the decisions render
has earned its include line in AGENTS.md, not before.

### 6. glossary, domain, taxonomy

Low marginal value now. The Domain section of AGENTS.md is a glossary
with relationships, hand-written and small. These earn a schema when
the vocabulary outgrows a page, or when the rules migration above pulls
the Domain section along with it.

## How to try one cheaply

The architecture makes a trial cheap. The log already holds the
experience, so a new schema can be folded from old events before any
writer is taught to fill it:

1. Add the schema to `SCHEMAS`, with its node and edge kinds.
2. Run `percept reflect` with a prompt naming that map, over the
   sessions this branch has already recorded. The model builds the
   map from past experience, citing the events it found.
3. Read the render in `.percept/<map>.md`. If it holds what the
   operation needs, teach a writer the write moment. If it does not,
   the kind is wrong, and the cost was one reflect turn.

Score a trial the way `experiment/claude-code` scores the decisions
map: correct answers to the operation's question, calls per question,
and whether the model treats the map as an index or as the truth.
