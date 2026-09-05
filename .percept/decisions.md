# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand.

## question
- "Where does the event log live?"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "Where should the command suggestion list render?"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Which key accepts a highlighted suggestion?"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f

## decision
- "one log under ~/.percept": why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "anchored above the input box": why: "input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Tab only": why: "Enter keeps its existing submit/execute behavior, so opening the dropdown doesn't change what Enter does"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f

## option
- "percept.jsonl in the working directory"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "one log under ~/.percept"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "centered popup like the existing ModelsMenu"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "anchored above the input box"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Tab only"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Enter only"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "both Tab and Enter"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f

## edges
- decision "one log under ~/.percept" resolves question "Where does the event log live?"
- decision "anchored above the input box" resolves question "Where should the command suggestion list render?"
- decision "Tab only" resolves question "Which key accepts a highlighted suggestion?"
