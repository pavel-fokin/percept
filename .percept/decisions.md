# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. Questions in the order they were raised, grouped under the prompt that raised them, each with its decision. Options and evidence: `percept maps show decisions --around 'question:<name>'`. What changed lately: `percept maps show decisions --since 1d`.

## 2026-09-05 · 01a0705c-72fd-70b1-a4c6-0408b12bd8d5
- "Where does the event log live?"
  decision "one log under ~/.percept": why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"

## 2026-09-05 · 01a070cc-c462-76d2-b536-3eae30e28f0f
- "Where should the command suggestion list render?"
  decision "anchored above the input box": why: "input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex"
- "Which key accepts a highlighted suggestion?"
  decision "Tab only": why: "Enter keeps its existing submit/execute behavior, so opening the dropdown doesn't change what Enter does"

## 2026-09-05 · 01a07195-c647-7f70-87ee-fb8f9faacdd8
- "Where should a dev build's event log default to when PERCEPT_HOME is unset?"
  decision "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept": why: "install.sh copies the binary to ~/.percept/bin, outside target/, so the check separates a repo build from an installed one without a build-time flag; PERCEPT_HOME still overrides either case"
  source 01a07195-da7b-72e3-9160-1577b943f1c4

## 2026-09-05 · 01a072dd-7ffc-7112-94d3-c7137849d2f2
- "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?"
  decision "accounts/fireworks/models/glm-5p3": why: "one static model like OPENAI_MODEL; Fireworks confirms Function Calling supported, no separate reasoning output"
- "How should the Fireworks provider be built?"
  decision "new Fireworks struct with its own wire parser": why: "Fireworks speaks classic /chat/completions SSE, not OpenAi's /responses shape"
- "Where do the Fireworks base URL and API key come from?"
  decision "hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var": why: "follows OPENAI_URL/OPENAI_KEY_VAR pattern: not env-configurable URL, eager key check in build_model, lenient in build_catalog"

## 2026-09-06 · 01a07544-9b9d-7ca0-bfea-9cb27dfe4b60
- "How does the decisions render stay readable as it grows?"
  decision "questions in first-seen order, grouped by the prompt that raised them, decision inline": why: "the raising prompt never changes, so a question never moves; the settling prompt moves when a decision is superseded, and the decision names its own prompt when it differs"
  source 01a07563-a7f6-7dc0-8504-a47276f2e3e9
  was "questions in first-seen order, grouped by the prompt that settled them, decision inline"
- "How is a decision corrected?"
  decision "add the new decision with a supersedes edge to the old one": why: "the old landmark stays one hop away and leaves the headlines; a removal makes the reader lose a point of reference"
- "Who may remove a node the user wrote?"
  decision "only the user; revise_map refuses and says to supersede": why: "a user-written node is the human's point of reference in a shared map; the model may attach edges to it but not take it away"
- "How does a reader see what changed in a map since they last looked?"
  decision "percept maps show <map> --since <time>": why: "same values and parser as events search --since; a fold, so it costs nothing; the change budget made visible without growing the render"
- "How does the model know which maps exist and when to open one?"
  decision "one catalogue line per map: purpose, size, last change": why: "the purpose is a fact of the schema, not the model's guess; size and last change say whether opening it is worth a call"
- "How is the node count of the decisions map kept down?"
  decision "hide options and evidence from the render": why: "dropping a kind would orphan eleven recorded nodes; the reader's cost is the render, and --around keeps the rest reachable; revisit after the questions experiment runs against a flatter schema"
- "How do --around and --since combine on maps show?"
  decision "--around cuts first, --since cuts what is left": why: "reads as what changed near this node; the other order would drop old edges before the neighbourhood is walked"
- "How does an option point at the question it was weighed for?"
  decision "an answers edge from the option to the question": why: "--around a question follows edges, not sources, so without it the render's pointer to --around reached no option"
