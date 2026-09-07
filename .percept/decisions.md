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

## 2026-09-06 · 01a0762d-7a6e-73a1-a7e9-ba4e182871d1
- "How does the model select a fragment of a map?"
  decision "read_map takes around, depth, since, and kinds, and opens with a line counting what the cut left out": why: "the CLI already had the cut; the model loop had none, so it read every node; the counts say how much was left out and how many edges cross the cut, taken from the codex/shared-maps branch"
- "How is the decisions map rendered for a reader?"
  decision "Markdown lines: one per question, one per decision": why: "the user was not sure about HTML; a line per node reads in a diff and in AGENTS.md, and the grouping needs no node the model has to invent"
- "Where do Claude Code and Codex find the shared instructions, skills, and capture hooks?"
  decision "any agent: one client-neutral body, and per client only a thin adapter that points at it": why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  source 01a07641-6411-7812-9878-09fc6a5f2e06
  was "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"
- "Where does a session start when it needs a map?"
  decision ".percept/index.md: one row per map with its use, origin, and entry point": why: "taken from codex/shared-maps; the catalogue line serves the model mid-turn, the index serves whoever opens the repo"

## 2026-09-06 · 01a07641-6411-7812-9878-09fc6a5f2e06
- "Which coding agents must this repo's percept setup serve?"
  decision "any agent: one client-neutral body, and per client only a thin adapter that points at it": why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  was "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"

## 2026-09-06 · 01a0767e-1c8a-7430-8dde-2d8d028c28ba
- "Who is the actor when the plan skill records a decision?"
  decision "model, through --actor model on the percept maps write verbs; the default stays user": why: "the plan skill is the agent writing; a human at the terminal is still user without a flag; the 74 nodes recorded before stay user, history does not move"
- "When is read_map offered to the model?"
  decision "always, in every map shape": why: "a cut around one node is worth a call even when the whole map is in the prompt; one tool fewer to reason about"
- "What makes an option worth a node?"
  decision "only an alternative that lost, and it says why: an option without a why property is refused": why: "the rule lives in the shared write path, so the CLI and revise_map both enforce it and the history of options without a reason still folds"
- "Where do the review passes live?"
  decision "in the workflow text, not as shared skills: each client uses its own review tooling, and CLAUDE.md names Claude Code's": why: "the core flow stays general; a client-specific file carries what only that client can do"
- "Which values does since take on the model's tools?"
  decision "the same as the CLI: ISO-8601 or Nd, Nh, Nm back from now, through one parser": why: "one meaning for since wherever it is typed"

## 2026-09-06 · 01a076df-b938-7ad1-b81b-42093e40f682
- "How does percept find the project root, and what happens when there is none?" (model)
  decision "Walk up from cwd for .git or .percept; stop at $HOME and the filesystem root; error when neither is found" (model): why: "matches git and cargo; the ceiling stops a stray ~/.git or the real ~/.percept from turning into a home-wide code-map walk that also trips macOS's protected folders"

## 2026-09-06 · 01a076c5-08cd-7b71-a036-cf1afda93dc7
- "Does install.sh copy the binary into ~/.percept/bin or symlink it?" (model)
  decision "Copy it with install -m 755, and also into ~/.local/bin or ~/bin when ~/.percept/bin is not on PATH" (model): why: "the binary must sit outside target/ for is_dev_build to tell an install from a cargo build (01a07195); the extra copy into a PATH dir lets percept resolve without editing PATH"
  source 01a076e9-5ddd-7221-90f5-4b23e7b57850

## 2026-09-06 · 01a0786b-a16d-7c52-9aaa-d4eb9b4ce16a
- "Which of a project's events should percept-tui replay as its own conversation?" (model)
  decision "conversational events count only from percept-tui's own source; a map mutation counts from any source in the project" (model): why: "to_messages already replayed another client's message.received/tool.called as this session's own dialogue, since agent-hook.py stamps the same event kinds for claude-code and codex; filtering self.events by source.name alone would also drop other sources' node/edge events from the fold, hiding decisions the plan skill recorded during a Claude Code session"

## 2026-09-07 · 01a07ab0-7838-7843-a5d2-0b456b01eb44
- "How are the coding tools switched on?" (model)
  decision "PERCEPT_TOOLS=code adds them beside the map tools; the default, maps, is today's behaviour" (model): why: "follows PERCEPT_MAPS: one env var read at the entrypoint, nothing else moves"
- "What are the coding tools named?" (model)
  decision "read_file, write_file, edit_file, list_files, find_files, grep_files, bash" (model): why: "verb_noun like read_event and read_map, so a bare read is never confused with them; no sed tool, edit_file covers it"
- "Which tool calls ask the user before they run?" (model)
  decision "write_file, edit_file and bash ask; the rest run; percept ask declines an ask unless --yes" (model): why: "reads are routine, mutation and arbitrary commands are not; a headless turn has no one to ask"
- "How many tool calls may a coding turn make?" (model)
  decision "50 with PERCEPT_TOOLS=code; 5 stays for maps" (model): why: "a coding task reads several files before one edit; five ends it mid-read"
- "How is a coding turn undone?" (model)
  decision "a commit at refs/percept/snapshots/<prompt id> before each prompt when the coding tools are on; /undo restores the last one" (model): why: "scratch refs leave the branch and index alone; version control is the undo the user already knows; the id ties it to the prompt event"
