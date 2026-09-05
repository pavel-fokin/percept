# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand.

## question
- "Where does the event log live?"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "Where should the command suggestion list render?"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Which key accepts a highlighted suggestion?"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Where should a dev build's event log default to when PERCEPT_HOME is unset?"
  sources: 01a07195-c647-7f70-87ee-fb8f9faacdd8
- "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "How should the Fireworks provider be built?"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "Where do the Fireworks base URL and API key come from?"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2

## decision
- "one log under ~/.percept": why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"
  sources: 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "anchored above the input box": why: "input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Tab only": why: "Enter keeps its existing submit/execute behavior, so opening the dropdown doesn't change what Enter does"
  sources: 01a070cc-c462-76d2-b536-3eae30e28f0f
- "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept": why: "install.sh copies the binary to ~/.percept/bin, outside target/, so the check separates a repo build from an installed one without a build-time flag; PERCEPT_HOME still overrides either case"
  sources: 01a07195-da7b-72e3-9160-1577b943f1c4
- "accounts/fireworks/models/glm-5p3": why: "one static model like OPENAI_MODEL; Fireworks confirms Function Calling supported, no separate reasoning output"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "new Fireworks struct with its own wire parser": why: "Fireworks speaks classic /chat/completions SSE, not OpenAi's /responses shape"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var": why: "follows OPENAI_URL/OPENAI_KEY_VAR pattern: not env-configurable URL, eager key check in build_model, lenient in build_catalog"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2

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
- "always ~/.percept, PERCEPT_HOME is the only override"
  sources: 01a07195-c647-7f70-87ee-fb8f9faacdd8
- "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept"
  sources: 01a07195-da7b-72e3-9160-1577b943f1c4
- "new Fireworks struct with its own wire parser"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "reuse OpenAi struct with a different base URL"
  sources: 01a072dd-7ffc-7112-94d3-c7137849d2f2

## edges
- decision "one log under ~/.percept" resolves question "Where does the event log live?"
- decision "anchored above the input box" resolves question "Where should the command suggestion list render?"
- decision "Tab only" resolves question "Which key accepts a highlighted suggestion?"
- decision "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept" resolves question "Where should a dev build's event log default to when PERCEPT_HOME is unset?"
- decision "accounts/fireworks/models/glm-5p3" resolves question "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?"
- decision "new Fireworks struct with its own wire parser" resolves question "How should the Fireworks provider be built?"
- decision "hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var" resolves question "Where do the Fireworks base URL and API key come from?"
