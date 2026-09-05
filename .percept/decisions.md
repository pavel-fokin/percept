# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand.

## question
- "Where does the event log live?"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5

## decision
- "one log under ~/.percept": why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5

## option
- "percept.jsonl in the working directory"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "one log under ~/.percept"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5

## edges
- decision "one log under ~/.percept" resolves question "Where does the event log live?"
