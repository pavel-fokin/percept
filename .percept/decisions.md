# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. A `## contents` list, then every question at `##` in the order it was raised - its decision, what that decision replaced (`was`), and the alternatives weighed against it. Evidence and full detail: `percept maps show decisions --around 'question:<name>'`. What changed lately: `percept maps show decisions --since 1d`.

## contents
- "Where does the event log live?" · 2026-09-05
- "Where should the command suggestion list render?" · 2026-09-05
- "Which key accepts a highlighted suggestion?" · 2026-09-05
- "Where should a dev build's event log default to when PERCEPT_HOME is unset?" · 2026-09-05
- "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?" · 2026-09-05
- "How should the Fireworks provider be built?" · 2026-09-05
- "Where do the Fireworks base URL and API key come from?" · 2026-09-05
- "How does the decisions render stay readable as it grows?" · 2026-09-06
- "How is a decision corrected?" · 2026-09-06
- "Who may remove a node the user wrote?" · 2026-09-06
- "How does a reader see what changed in a map since they last looked?" · 2026-09-06
- "How does the model know which maps exist and when to open one?" · 2026-09-06
- "How is the node count of the decisions map kept down?" · 2026-09-06
- "How do --around and --since combine on maps show?" · 2026-09-06
- "How does an option point at the question it was weighed for?" · 2026-09-06
- "How does the model select a fragment of a map?" · 2026-09-06
- "How is the decisions map rendered for a reader?" · 2026-09-06
- "Where do Claude Code and Codex find the shared instructions, skills, and capture hooks?" · 2026-09-06
- "Where does a session start when it needs a map?" · 2026-09-06
- "Which coding agents must this repo's percept setup serve?" · 2026-09-06
- "Who is the actor when the plan skill records a decision?" · 2026-09-06
- "When is read_map offered to the model?" · 2026-09-06
- "What makes an option worth a node?" · 2026-09-06
- "Where do the review passes live?" · 2026-09-06
- "Which values does since take on the model's tools?" · 2026-09-06
- "How does percept find the project root, and what happens when there is none?" (model) · 2026-09-06
- "Does install.sh copy the binary into ~/.percept/bin or symlink it?" (model) · 2026-09-06
- "Which of a project's events should percept-tui replay as its own conversation?" (model) · 2026-09-06
- "How are the coding tools switched on?" (model) · 2026-09-07
- "What are the coding tools named?" (model) · 2026-09-07
- "Which tool calls ask the user before they run?" (model) · 2026-09-07
- "How many tool calls may a coding turn make?" (model) · 2026-09-07
- "How is a coding turn undone?" (model) · 2026-09-07
- "How does a user stop approving every tool call?" (model) · 2026-09-07
- "How does a coding turn learn the project's conventions?" (model) · 2026-09-07
- "Where is future work tracked?" (model) · 2026-09-07
- "Where does a done task go in the tasks render?" (model) · 2026-09-07
- "How does the map render stay scannable as groups pile up?" (model) · 2026-09-07
- "Are rejected options shown in the decisions render?" (model) · 2026-09-07
- "How is a map read as Markdown from the CLI?" (model) · 2026-09-07
- "How does a long why read in the render?" (model) · 2026-09-07
- "How should GitHub issues integrate with the tasks and decisions maps?" (model) · 2026-09-07
- "Can the model reach the code map through read_map?" (model) · 2026-09-07
- "Which gpt-5.6 models does the OpenAI catalog offer, and how are they picked?" (model) · 2026-09-07
- "How are OpenAI reasoning-effort levels exposed in the catalog?" (model) · 2026-09-07
- "How does a reader learn a map's node and edge kinds?" (model) · 2026-09-07
- "What does maps list --format md render per map?" (model) · 2026-09-07
- "Does AGENTS.md list the code map's node kinds?" (model) · 2026-09-07

## "Where does the event log live?"

- decision "one log under ~/.percept"
  why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"

## "Where should the command suggestion list render?"

- decision "anchored above the input box"
  why: "input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex"

## "Which key accepts a highlighted suggestion?"

- decision "Tab only"
  why: "Enter keeps its existing submit/execute behavior, so opening the dropdown doesn't change what Enter does"

## "Where should a dev build's event log default to when PERCEPT_HOME is unset?"

- decision "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept"
  why: "install.sh copies the binary to ~/.percept/bin, outside target/, so the check separates a repo build from an installed one without a build-time flag; PERCEPT_HOME still overrides either case"
  source 01a07195-da7b-72e3-9160-1577b943f1c4

## "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?"

- decision "accounts/fireworks/models/glm-5p3"
  why: "one static model like OPENAI_MODEL; Fireworks confirms Function Calling supported, no separate reasoning output"

## "How should the Fireworks provider be built?"

- decision "new Fireworks struct with its own wire parser"
  why: "Fireworks speaks classic /chat/completions SSE, not OpenAi's /responses shape"

## "Where do the Fireworks base URL and API key come from?"

- decision "hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var"
  why: "follows OPENAI_URL/OPENAI_KEY_VAR pattern: not env-configurable URL, eager key check in build_model, lenient in build_catalog"

## "How does the decisions render stay readable as it grows?"

- decision "questions in first-seen order, grouped by the prompt that raised them, decision inline"
  why: "the raising prompt never changes, so a question never moves; the settling prompt moves when a decision is superseded, and the decision names its own prompt when it differs"
  source 01a07563-a7f6-7dc0-8504-a47276f2e3e9
  was "questions in first-seen order, grouped by the prompt that settled them, decision inline"
- weighed "one section per node kind, every node listed"

## "How is a decision corrected?"

- decision "add the new decision with a supersedes edge to the old one"
  why: "the old landmark stays one hop away and leaves the headlines; a removal makes the reader lose a point of reference"
- weighed "remove the old node and add a new one"

## "Who may remove a node the user wrote?"

- decision "only the user; revise_map refuses and says to supersede"
  why: "a user-written node is the human's point of reference in a shared map; the model may attach edges to it but not take it away"
- weighed "any writer"

## "How does a reader see what changed in a map since they last looked?"

- decision "percept maps show <map> --since <time>"
  why: "same values and parser as events search --since; a fold, so it costs nothing; the change budget made visible without growing the render"
- weighed "a changelog section in the render"

## "How does the model know which maps exist and when to open one?"

- decision "one catalogue line per map: purpose, size, last change"
  why: "the purpose is a fact of the schema, not the model's guess; size and last change say whether opening it is worth a call"
- weighed "the whole map in every prompt, as before"

## "How is the node count of the decisions map kept down?"

- decision "show options inline as 'weighed:' lines under the question; evidence stays hidden" (model)
  why: "a bare decision reads thin without the alternatives it beat; evidence is still one --around away"
  source 01a07bbc-9315-7c53-b931-186847df7c96
  was "hide options and evidence from the render"
- weighed "drop option as a node kind"
- weighed "flag decisions whose subject left the code map as stale"

## "How do --around and --since combine on maps show?"

- decision "--around cuts first, --since cuts what is left"
  why: "reads as what changed near this node; the other order would drop old edges before the neighbourhood is walked"

## "How does an option point at the question it was weighed for?"

- decision "an answers edge from the option to the question"
  why: "--around a question follows edges, not sources, so without it the render's pointer to --around reached no option"
- weighed "by sharing the question's source prompt only"

## "How does the model select a fragment of a map?"

- decision "read_map takes around, depth, since, and kinds, and opens with a line counting what the cut left out"
  why: "the CLI already had the cut; the model loop had none, so it read every node; the counts say how much was left out and how many edges cross the cut, taken from the codex/shared-maps branch"
- weighed "read_map returns the whole map; the model reads it all"

## "How is the decisions map rendered for a reader?"

- decision "Markdown lines: one per question, one per decision"
  why: "the user was not sure about HTML; a line per node reads in a diff and in AGENTS.md, and the grouping needs no node the model has to invent"
- weighed "HTML details blocks under an overview of commitment nodes"

## "Where do Claude Code and Codex find the shared instructions, skills, and capture hooks?"

- decision "any agent: one client-neutral body, and per client only a thin adapter that points at it"
  why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  source 01a07641-6411-7812-9878-09fc6a5f2e06
  was "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"

## "Where does a session start when it needs a map?"

- decision ".percept/index.md: one row per map with its use, origin, and entry point"
  why: "taken from codex/shared-maps; the catalogue line serves the model mid-turn, the index serves whoever opens the repo"

## "Which coding agents must this repo's percept setup serve?"

- decision "any agent: one client-neutral body, and per client only a thin adapter that points at it"
  why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  was "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"
- weighed "Claude Code and Codex, each with its own copy of the setup"

## "Who is the actor when the plan skill records a decision?"

- decision "model, through --actor model on the percept maps write verbs; the default stays user"
  why: "the plan skill is the agent writing; a human at the terminal is still user without a flag; the 74 nodes recorded before stay user, history does not move"
- weighed "user, as the CLI has always committed"
  why: "an agent recording on the user's behalf is not the user; every node was user, so the guard and the (model) mark carried nothing"

## "When is read_map offered to the model?"

- decision "always, in every map shape"
  why: "a cut around one node is worth a call even when the whole map is in the prompt; one tool fewer to reason about"
- weighed "only in the headlines and tool shapes"
  why: "in the default prompt shape the model got the whole map and no way to cut it, so the fragment selection was unreachable by default"

## "What makes an option worth a node?"

- decision "only an alternative that lost, and it says why: an option without a why property is refused"
  why: "the rule lives in the shared write path, so the CLI and revise_map both enforce it and the history of options without a reason still folds"
- weighed "record every option weighed, the pick included"
  why: "32 options for 20 questions, most of them the pick repeated as a decision or a name with no reason; less is more"
- weighed "drop the option kind"
  why: "a real rejected alternative with its reason is the one thing the decision's own why cannot carry; the reason is what stops a later session retrying it"

## "Where do the review passes live?"

- decision "in the workflow text, not as shared skills: each client uses its own review tooling, and CLAUDE.md names Claude Code's"
  why: "the core flow stays general; a client-specific file carries what only that client can do"
- weighed "shared code-review and simplify skills under .agents/skills"
  why: "Claude Code's built-in skills of the same names shadow them, so the two clients ran different passes; other clients may have no skill mechanism at all"

## "Which values does since take on the model's tools?"

- decision "the same as the CLI: ISO-8601 or Nd, Nh, Nm back from now, through one parser"
  why: "one meaning for since wherever it is typed"
- weighed "ISO-8601 only on the tools, since the model is told the time"
  why: "two parsers for one word, and a model that writes 1d wasted a call"

## "How does percept find the project root, and what happens when there is none?" (model)

- decision "Walk up from cwd for .git or .percept; stop at $HOME and the filesystem root; error when neither is found" (model)
  why: "matches git and cargo; the ceiling stops a stray ~/.git or the real ~/.percept from turning into a home-wide code-map walk that also trips macOS's protected folders"
- weighed "Keep the fallback that walks the current directory when no .git is found" (model)
  why: "from $HOME it walks the whole home directory: slow, and permission errors on macOS's protected Desktop, Photos and Music"
- weighed "Guard only the code map, leave other commands working from any directory" (model)
  why: "nothing percept does is useful outside a project, so one discovery rule beats a special case"

## "Does install.sh copy the binary into ~/.percept/bin or symlink it?" (model)

- decision "Copy it with install -m 755, and also into ~/.local/bin or ~/bin when ~/.percept/bin is not on PATH" (model)
  why: "the binary must sit outside target/ for is_dev_build to tell an install from a cargo build (01a07195); the extra copy into a PATH dir lets percept resolve without editing PATH"
  source 01a076e9-5ddd-7221-90f5-4b23e7b57850
- weighed "Symlink target/release/percept into ~/.percept/bin so a rebuild updates the install" (model)
  why: "the agent hook resolves the link before exec and Linux current_exe() resolves /proc/self/exe, so is_dev_build sees a target/release path and routes the install's events to <checkout>/.percept; tried and reverted 2026-09-06"

## "Which of a project's events should percept-tui replay as its own conversation?" (model)

- decision "conversational events count only from percept-tui's own source; a map mutation counts from any source in the project" (model)
  why: "to_messages already replayed another client's message.received/tool.called as this session's own dialogue, since agent-hook.py stamps the same event kinds for claude-code and codex; filtering self.events by source.name alone would also drop other sources' node/edge events from the fold, hiding decisions the plan skill recorded during a Claude Code session"
- weighed "filter self.events to source.name and source.path at load time" (model)
  why: "simpler, but strips other sources' NodeAdded/EdgeAdded events out of the map fold too, so decisions and other map nodes recorded via Claude Code or the CLI would silently vanish from the TUI's maps"

## "How are the coding tools switched on?" (model)

- decision "the TUI defaults to code tools only in a git checkout; headless and non-git TUI default to maps; PERCEPT_TOOLS overrides either" (model)
  why: "the code toolset's undo is a git snapshot, so an unconditional default broke the TUI in a .percept-only project - a layout checkout_root supports; resolve_toolset now picks code only where a git checkout exists"
  source 01a07c5e-04b7-7860-9c9c-07b815f05867
  was "the TUI defaults to code tools while headless commands default to maps; PERCEPT_TOOLS overrides either" (model)
  was "PERCEPT_TOOLS=code adds them beside the map tools; the default, maps, is today's behaviour" (model)
- weighed "always on, in every turn" (model)
  why: "every turn, reflect included, would gain the tree and spend its tool calls on it"

## "What are the coding tools named?" (model)

- decision "read_file, write_file, edit_file, list_files, find_files, grep_files, bash" (model)
  why: "verb_noun like read_event and read_map, so a bare read is never confused with them; no sed tool, edit_file covers it"
- weighed "shell, running sh -c" (model)
  why: "more abstract, but models are trained on a tool named bash and write bashisms; the tool now runs bash -c so the name is true"

## "Which tool calls ask the user before they run?" (model)

- decision "write_file, edit_file and bash ask; the rest run; percept ask declines an ask unless --yes" (model)
  why: "reads are routine, mutation and arbitrary commands are not; a headless turn has no one to ask"

## "How many tool calls may a coding turn make?" (model)

- decision "50 with PERCEPT_TOOLS=code; 5 stays for maps" (model)
  why: "a coding task reads several files before one edit; five ends it mid-read"

## "How is a coding turn undone?" (model)

- decision "a commit at refs/percept/snapshots/<prompt id> before each prompt when the coding tools are on; /undo restores the last one" (model)
  why: "scratch refs leave the branch and index alone; version control is the undo the user already knows; the id ties it to the prompt event"
- weighed "an overlay filesystem the shell writes through" (model)
  why: "a shell command runs on the real disk, so the overlay goes stale or must be written out before every command"

## "How does a user stop approving every tool call?" (model)

- decision "a on the approval row runs the call and every later call of that tool this session; y runs once, n declines" (model)
  why: "one answer per tool per session; nothing is committed to the log, so the next session asks again"
- weighed "remember each approved command text" (model)
  why: "a coding turn varies the command every time, so it would ask as often as before"

## "How does a coding turn learn the project's conventions?" (model)

- decision "AGENTS.md at the checkout root goes into the system prompt every round under PERCEPT_TOOLS=code; absent, nothing is sent" (model)
  why: "the coding agent wrote banner comments, an overlong subject and skipped review because it never saw the rules; a chat over the log has no tree to follow them in and pays nothing"
- weighed "read CLAUDE.md" (model)
  why: "one client's file; percept serves any client, and this repo keeps its body in AGENTS.md with CLAUDE.md as an adapter"

## "Where is future work tracked?" (model)

- decision "a tasks map: task and outcome nodes; resolves, blocks and supersedes edges; every task says why" (model)
  why: "resolves is the word decisions already uses, so a reader learns one vocabulary; a task without a why is a todo nobody can weigh"
- weighed "a task kind on the decisions map" (model)
  why: "an open question is a design item; work is a different reasoning operation, what is next and what it waits on, and earns its own map"

## "Where does a done task go in the tasks render?" (model)

- decision "a Done section below the open tasks, each with its outcome" (model)
  why: "findable without a query; the open list stays the part a session reads first"
- weighed "hidden, reached only with --around" (model)
  why: "keeps the render short, but a resolved task is a landmark a reader may still steer by, and stability weighs more than compactness here"

## "How does the map render stay scannable as groups pile up?" (model)

- decision "head every question at ## with a flat contents list; drop the raising-prompt grouping and the short id" (model)
  why: "the raising prompt is usually 'Approved'; its id names nothing and cannot be looked up; first-seen order alone keeps a question from moving, and one ## per question makes the file its own outline"
  source 01a07bf0-1126-7343-9b34-aff73a7c9a38
  was "head each group with its first node's text plus a short prompt id, and open with a ## contents list of every group" (model)
- weighed "keep the date-and-uuid group heading" (model)
  why: "the raising prompt is often 'Approved' or 'agreed. proceed'; the id names nothing a reader can scan for"

## "Are rejected options shown in the decisions render?" (model)

- decision "show options inline as 'weighed:' lines under the question; evidence stays hidden" (model)
  why: "a bare decision reads thin without the alternatives it beat; evidence is still one --around away"
  was "hide options and evidence from the render"

## "How is a map read as Markdown from the CLI?" (model)

- decision "--format json|md on maps show and maps list, default json, md renders store::markdown" (model)
  why: "additive: existing callers unchanged; md is opt-in and shares the renderer that writes .percept/*.md"
- weighed "flip the maps show default to Markdown" (model)
  why: "breaks the code map's documented jq pipelines and every caller that parses the JSONL; churn with no gain for them"

## "How does a long why read in the render?" (model)

- decision "each property on its own line under its node, never appended to the name" (model)
  why: "a decision plus a long why was one unreadable line; the name and the reason are different things and belong on different lines"

## "How should GitHub issues integrate with the tasks and decisions maps?" (model)

note: "Not decided. Four models sketched: A - issue as a link, a task node carries issue:N, one-way push creates/closes, percept stays truth; B - issue as an event source, a webhook publishes issue.opened/closed under source github and a fold derives tasks; C - bidirectional sync; D - a GithubIssues MapRenderer beside MarkdownFiles. Leaning A plus a thin one-way push. Keep decisions percept-native; keep GitHub a strict projection so it is not a third party in the human/agent merge. Full tradeoffs in the 2026-09-07 session."
- open

## "Can the model reach the code map through read_map?" (model)

- decision "read_map serves the code map too, dispatched by map name, offered in every turn" (model)
  why: "the model had one ergonomic tool for decisions and tasks and a bash incantation for code, so it grepped; read_map now dispatches code to the working-tree walk through a MapReader port so store keeps no sideways dep; --since on code is refused as in the CLI"
- weighed "a separate read_code tool beside read_map" (model)
  why: "percept keeps one map-reading tool on purpose (When is read_map offered: one tool fewer to reason about); a second tool splits that"
- weighed "only point the Derived error at percept maps show code" (model)
  why: "leaves the friction that caused the miss: the model still shells out, learns the CLI syntax, and parses JSONL instead of calling a typed tool"

## "Which gpt-5.6 models does the OpenAI catalog offer, and how are they picked?" (model)

- decision "terra and sol join the OpenAI catalog list; luna stays what main and headless build" (model)
  why: "openai.rs already knows the shape of all three (1.05M window, thinking); only OPENAI_MODELS was one entry, so the /models picker never offered terra or sol. OPENAI_MODEL stays luna, the default with no picker."
- weighed "an OPENAI_MODEL env var so headless runs can pick terra or sol" (model)
  why: "each hosted provider already leans on one static model (see Fireworks); a headless run has no picker, and one fixed default keeps startup predictable"
- weighed "a per-model reasoning-effort table for the gpt-5.6 family" (model)
  why: "no per-model effort mechanism exists; effort stays one global knob and all three share the 1.05M window, so global low is a safe floor"

## "How are OpenAI reasoning-effort levels exposed in the catalog?" (model)

- decision "ModelDescriptor carries low, medium, and high effort capabilities for OpenAI models" (model)
  why: "the catalog tells callers which effort levels a listed model supports; the configured global effort remains what build sends"

## "How does a reader learn a map's node and edge kinds?" (model)

- decision "each kind carries a gloss on its Schema, shown by maps list --format md and the read_map response" (model)
  why: "the gloss lives once beside the kind; both agent surfaces render it; AGENTS.md stops carrying a list that drifts"
- weighed "a maps describe <map> subcommand" (model)
  why: "maps list --format md already prints one section per map; a third verb to show what a second verb can"
- weighed "keep the kind list in AGENTS.md prose" (model)
  why: "it duplicated code.md, drifted, and this session's model read package:providers by the wrong meaning"

## "What does maps list --format md render per map?" (model)

- decision "a per-map section: purpose, counts, kinds with glosses, and one live example node and edge line" (model)
  why: "the section holds what a table cannot; the example anchors the kind names to real strings"
- weighed "keep the single table, add columns for kinds" (model)
  why: "a table cell cannot hold a kind-and-gloss list plus a JSONL example line"

## "Does AGENTS.md list the code map's node kinds?" (model)

- decision "no; the code row points at percept maps list, and the preamble notes code keys internal code by file path" (model)
  why: "the Package column trained the model to try package:providers this session; the kinds now live on the Schema"
