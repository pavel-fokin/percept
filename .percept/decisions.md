# decisions

Folded from the percept log for this project and rerendered on every write. Change it with `percept maps`, not by hand. A `## contents` list, then every question at `##` in the order it was raised - its decision, what that decision replaced (`was`), and the alternatives weighed against it. Evidence and full detail: `percept maps show decisions --around 'question:<name>'`. What changed lately: `percept maps show decisions --since 1d`.

## contents
- q1 "Where does the event log live?" · 2026-09-05
- q2 "Where should the command suggestion list render?" · 2026-09-05
- q3 "Which key accepts a highlighted suggestion?" · 2026-09-05
- q4 "Where should a dev build's event log default to when PERCEPT_HOME is unset?" · 2026-09-05
- q5 "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?" · 2026-09-05
- q6 "How should the Fireworks provider be built?" · 2026-09-05
- q7 "Where do the Fireworks base URL and API key come from?" · 2026-09-05
- q8 "How does the decisions render stay readable as it grows?" · 2026-09-06
- q9 "How is a decision corrected?" · 2026-09-06
- q10 "Who may remove a node the user wrote?" · 2026-09-06
- q11 "How does a reader see what changed in a map since they last looked?" · 2026-09-06
- q12 "How does the model know which maps exist and when to open one?" · 2026-09-06
- q13 "How is the node count of the decisions map kept down?" · 2026-09-06
- q14 "How do --around and --since combine on maps show?" · 2026-09-06
- q15 "How does an option point at the question it was weighed for?" · 2026-09-06
- q16 "How does the model select a fragment of a map?" · 2026-09-06
- q17 "How is the decisions map rendered for a reader?" · 2026-09-06
- q18 "Where do Claude Code and Codex find the shared instructions, skills, and capture hooks?" · 2026-09-06
- q19 "Where does a session start when it needs a map?" · 2026-09-06
- q20 "Which coding agents must this repo's percept setup serve?" · 2026-09-06
- q21 "Who is the actor when the plan skill records a decision?" · 2026-09-06
- q22 "When is read_map offered to the model?" · 2026-09-06
- q23 "What makes an option worth a node?" · 2026-09-06
- q24 "Where do the review passes live?" · 2026-09-06
- q25 "Which values does since take on the model's tools?" · 2026-09-06
- q26 "How does percept find the project root, and what happens when there is none?" (model) · 2026-09-06
- q27 "Does install.sh copy the binary into ~/.percept/bin or symlink it?" (model) · 2026-09-06
- q28 "Which of a project's events should percept-tui replay as its own conversation?" (model) · 2026-09-06
- q29 "How are the coding tools switched on?" (model) · 2026-09-07
- q30 "What are the coding tools named?" (model) · 2026-09-07
- q31 "Which tool calls ask the user before they run?" (model) · 2026-09-07
- q32 "How many tool calls may a coding turn make?" (model) · 2026-09-07
- q33 "How is a coding turn undone?" (model) · 2026-09-07
- q34 "How does a user stop approving every tool call?" (model) · 2026-09-07
- q35 "How does a coding turn learn the project's conventions?" (model) · 2026-09-07
- q36 "Where is future work tracked?" (model) · 2026-09-07
- q37 "Where does a done task go in the tasks render?" (model) · 2026-09-07
- q38 "How does the map render stay scannable as groups pile up?" (model) · 2026-09-07
- q39 "Are rejected options shown in the decisions render?" (model) · 2026-09-07
- q40 "How is a map read as Markdown from the CLI?" (model) · 2026-09-07
- q41 "How does a long why read in the render?" (model) · 2026-09-07
- q42 "How should GitHub issues integrate with the tasks and decisions maps?" (model) · 2026-09-07
- q43 "Can the model reach the code map through read_map?" (model) · 2026-09-07
- q44 "Which gpt-5.6 models does the OpenAI catalog offer, and how are they picked?" (model) · 2026-09-07
- q45 "How are OpenAI reasoning-effort levels exposed in the catalog?" (model) · 2026-09-07
- q46 "How does a reader learn a map's node and edge kinds?" (model) · 2026-09-07
- q47 "What does maps list --format md render per map?" (model) · 2026-09-07
- q48 "Does AGENTS.md list the code map's node kinds?" (model) · 2026-09-07
- q49 "How does the TUI select reasoning effort?" (model) · 2026-09-07
- q50 "What happens to reasoning effort when the model changes?" (model) · 2026-09-07
- q51 "Are the maps and code toolsets two harnesses?" (model) · 2026-09-07
- q52 "How far back does the model's history reach?" (model) · 2026-09-07
- q53 "What does the model see past the window?" (model) · 2026-09-07
- q54 "In what order does a request carry its sections?" (model) · 2026-09-07
- q55 "How does a user see what the model was shown?" (model) · 2026-09-07
- q56 "Where do design proposals live?" (model) · 2026-09-07
- q57 "How does the prompt stay within the model's window as the maps grow?" (model) · 2026-09-07
- q58 "How does a rendered map stay scoped to the branch it is committed in?" (model) · 2026-09-07
- q59 "Where does agreement between the human and the agent live?" (model) · 2026-09-08
- q60 "What is percept's core called?" (model) · 2026-09-08
- q61 "How is a human contour identified?" (model) · 2026-09-08
- q62 "Where does a map's schema live?" (model) · 2026-09-08
- q63 "What format is a schema file?" (model) · 2026-09-08
- q64 "How does a schema say which properties a node must carry?" (model) · 2026-09-08
- q65 "How does the hook command learn which event fired?" (model) · 2026-09-08
- q66 "How does a hook config line find the binary?" (model) · 2026-09-08
- q67 "Which percept commands does init allowlist for Claude Code?" (model) · 2026-09-08
- q68 "Where does the hook's turn state live?" (model) · 2026-09-08
- q69 "Where does percept init write a client's config?" (model) · 2026-09-08
- q70 "How does the hook find the checkout?" (model) · 2026-09-08
- q71 "When does the shared log outgrow a full-file fold, and what fixes it?" (model) · 2026-09-09
- q72 "How is a short id minted and displayed for a node?" (model) · 2026-09-09
- q73 "How does the model learn what changed since it last opened this project?" (model) · 2026-09-09

## q1 "Where does the event log live?"

- decision d1 "one log under ~/.percept"
  why: "PERCEPT_HOME also holds the binary; one log keeps cross-project search free; an event's source.path says which project"

## q2 "Where should the command suggestion list render?"

- decision d2 "anchored above the input box"
  why: "input sits between the transcript and the status bar, so suggestions render in that gap, matching Claude Code/Codex"

## q3 "Which key accepts a highlighted suggestion?"

- decision d3 "Tab only"
  why: "Enter keeps its existing submit/execute behavior, so opening the dropdown doesn't change what Enter does"

## q4 "Where should a dev build's event log default to when PERCEPT_HOME is unset?"

- decision d4 "detect target/debug or target/release via current_exe(), default those to <checkout>/.percept"
  why: "install.sh copies the binary to ~/.percept/bin, outside target/, so the check separates a repo build from an installed one without a build-time flag; PERCEPT_HOME still overrides either case"
  source 01a07195-da7b-72e3-9160-1577b943f1c4

## q5 "Which Fireworks model should PERCEPT_PROVIDER=fireworks build?"

- decision d5 "accounts/fireworks/models/glm-5p3"
  why: "one static model like OPENAI_MODEL; Fireworks confirms Function Calling supported, no separate reasoning output"

## q6 "How should the Fireworks provider be built?"

- decision d6 "new Fireworks struct with its own wire parser"
  why: "Fireworks speaks classic /chat/completions SSE, not OpenAi's /responses shape"

## q7 "Where do the Fireworks base URL and API key come from?"

- decision d7 "hardcoded https://api.fireworks.ai/inference/v1, FIREWORKS_API_KEY env var"
  why: "follows OPENAI_URL/OPENAI_KEY_VAR pattern: not env-configurable URL, eager key check in build_model, lenient in build_catalog"

## q8 "How does the decisions render stay readable as it grows?"

- decision d16 "questions in first-seen order, grouped by the prompt that raised them, decision inline"
  why: "the raising prompt never changes, so a question never moves; the settling prompt moves when a decision is superseded, and the decision names its own prompt when it differs"
  source 01a07563-a7f6-7dc0-8504-a47276f2e3e9
  was d8 "questions in first-seen order, grouped by the prompt that settled them, decision inline"
- weighed o12 "one section per node kind, every node listed"

## q9 "How is a decision corrected?"

- decision d9 "add the new decision with a supersedes edge to the old one"
  why: "the old landmark stays one hop away and leaves the headlines; a removal makes the reader lose a point of reference"
- weighed o14 "remove the old node and add a new one"

## q10 "Who may remove a node the user wrote?"

- decision d10 "only the user; revise_map refuses and says to supersede"
  why: "a user-written node is the human's point of reference in a shared map; the model may attach edges to it but not take it away"
- weighed o16 "any writer"

## q11 "How does a reader see what changed in a map since they last looked?"

- decision d11 "percept maps show <map> --since <time>"
  why: "same values and parser as events search --since; a fold, so it costs nothing; the change budget made visible without growing the render"
- weighed o18 "a changelog section in the render"

## q12 "How does the model know which maps exist and when to open one?"

- decision d12 "one catalogue line per map: purpose, size, last change"
  why: "the purpose is a fact of the schema, not the model's guess; size and last change say whether opening it is worth a call"
- weighed o20 "the whole map in every prompt, as before"

## q13 "How is the node count of the decisions map kept down?"

- decision d40 "show options inline as 'weighed:' lines under the question; evidence stays hidden" (model)
  why: "a bare decision reads thin without the alternatives it beat; evidence is still one --around away"
  source 01a07bbc-9315-7c53-b931-186847df7c96
  was d13 "hide options and evidence from the render"
- weighed o22 "drop option as a node kind"
- weighed o24 "flag decisions whose subject left the code map as stale"

## q14 "How do --around and --since combine on maps show?"

- decision d14 "--around cuts first, --since cuts what is left"
  why: "reads as what changed near this node; the other order would drop old edges before the neighbourhood is walked"

## q15 "How does an option point at the question it was weighed for?"

- decision d15 "an answers edge from the option to the question"
  why: "--around a question follows edges, not sources, so without it the render's pointer to --around reached no option"
- weighed o25 "by sharing the question's source prompt only"

## q16 "How does the model select a fragment of a map?"

- decision d17 "read_map takes around, depth, since, and kinds, and opens with a line counting what the cut left out"
  why: "the CLI already had the cut; the model loop had none, so it read every node; the counts say how much was left out and how many edges cross the cut, taken from the codex/shared-maps branch"
- weighed o27 "read_map returns the whole map; the model reads it all"

## q17 "How is the decisions map rendered for a reader?"

- decision d18 "Markdown lines: one per question, one per decision"
  why: "the user was not sure about HTML; a line per node reads in a diff and in AGENTS.md, and the grouping needs no node the model has to invent"
- weighed o29 "HTML details blocks under an overview of commitment nodes"

## q18 "Where do Claude Code and Codex find the shared instructions, skills, and capture hooks?"

- decision d21 "any agent: one client-neutral body, and per client only a thin adapter that points at it"
  why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  source 01a07641-6411-7812-9878-09fc6a5f2e06
  was d19 "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"

## q19 "Where does a session start when it needs a map?"

- decision d20 ".percept/index.md: one row per map with its use, origin, and entry point"
  why: "taken from codex/shared-maps; the catalogue line serves the model mid-turn, the index serves whoever opens the repo"

## q20 "Which coding agents must this repo's percept setup serve?"

- decision d21 "any agent: one client-neutral body, and per client only a thin adapter that points at it"
  why: "percept is for working with different coding agents and tools; instructions in AGENTS.md, skills in .agents/skills, capture in scripts/agent-hook.py, and a client folder holds only discovery metadata and the commands that call them, so a new agent costs an adapter, never a copy"
  was d19 "one body under .agents and scripts/agent-hook.py; .claude symlinks to it and .codex adapts it"
- weighed o31 "Claude Code and Codex, each with its own copy of the setup"

## q21 "Who is the actor when the plan skill records a decision?"

- decision d22 "model, through --actor model on the percept maps write verbs; the default stays user"
  why: "the plan skill is the agent writing; a human at the terminal is still user without a flag; the 74 nodes recorded before stay user, history does not move"
- weighed o33 "user, as the CLI has always committed"
  why: "an agent recording on the user's behalf is not the user; every node was user, so the guard and the (model) mark carried nothing"

## q22 "When is read_map offered to the model?"

- decision d23 "always, in every map shape"
  why: "a cut around one node is worth a call even when the whole map is in the prompt; one tool fewer to reason about"
- weighed o34 "only in the headlines and tool shapes"
  why: "in the default prompt shape the model got the whole map and no way to cut it, so the fragment selection was unreachable by default"

## q23 "What makes an option worth a node?"

- decision d24 "only an alternative that lost, and it says why: an option without a why property is refused"
  why: "the rule lives in the shared write path, so the CLI and revise_map both enforce it and the history of options without a reason still folds"
- weighed o35 "record every option weighed, the pick included"
  why: "32 options for 20 questions, most of them the pick repeated as a decision or a name with no reason; less is more"
- weighed o36 "drop the option kind"
  why: "a real rejected alternative with its reason is the one thing the decision's own why cannot carry; the reason is what stops a later session retrying it"

## q24 "Where do the review passes live?"

- decision d25 "in the workflow text, not as shared skills: each client uses its own review tooling, and CLAUDE.md names Claude Code's"
  why: "the core flow stays general; a client-specific file carries what only that client can do"
- weighed o37 "shared code-review and simplify skills under .agents/skills"
  why: "Claude Code's built-in skills of the same names shadow them, so the two clients ran different passes; other clients may have no skill mechanism at all"

## q25 "Which values does since take on the model's tools?"

- decision d26 "the same as the CLI: ISO-8601 or Nd, Nh, Nm back from now, through one parser"
  why: "one meaning for since wherever it is typed"
- weighed o38 "ISO-8601 only on the tools, since the model is told the time"
  why: "two parsers for one word, and a model that writes 1d wasted a call"

## q26 "How does percept find the project root, and what happens when there is none?" (model)

- decision d27 "Walk up from cwd for .git or .percept; stop at $HOME and the filesystem root; error when neither is found" (model)
  why: "matches git and cargo; the ceiling stops a stray ~/.git or the real ~/.percept from turning into a home-wide code-map walk that also trips macOS's protected folders"
- weighed o39 "Keep the fallback that walks the current directory when no .git is found" (model)
  why: "from $HOME it walks the whole home directory: slow, and permission errors on macOS's protected Desktop, Photos and Music"
- weighed o40 "Guard only the code map, leave other commands working from any directory" (model)
  why: "nothing percept does is useful outside a project, so one discovery rule beats a special case"

## q27 "Does install.sh copy the binary into ~/.percept/bin or symlink it?" (model)

- decision d28 "Copy it with install -m 755, and also into ~/.local/bin or ~/bin when ~/.percept/bin is not on PATH" (model)
  why: "the binary must sit outside target/ for is_dev_build to tell an install from a cargo build (01a07195); the extra copy into a PATH dir lets percept resolve without editing PATH"
  source 01a076e9-5ddd-7221-90f5-4b23e7b57850
- weighed o41 "Symlink target/release/percept into ~/.percept/bin so a rebuild updates the install" (model)
  why: "the agent hook resolves the link before exec and Linux current_exe() resolves /proc/self/exe, so is_dev_build sees a target/release path and routes the install's events to <checkout>/.percept; tried and reverted 2026-09-06"

## q28 "Which of a project's events should percept-tui replay as its own conversation?" (model)

- decision d29 "conversational events count only from percept-tui's own source; a map mutation counts from any source in the project" (model)
  why: "to_messages already replayed another client's message.received/tool.called as this session's own dialogue, since agent-hook.py stamps the same event kinds for claude-code and codex; filtering self.events by source.name alone would also drop other sources' node/edge events from the fold, hiding decisions the plan skill recorded during a Claude Code session"
- weighed o42 "filter self.events to source.name and source.path at load time" (model)
  why: "simpler, but strips other sources' NodeAdded/EdgeAdded events out of the map fold too, so decisions and other map nodes recorded via Claude Code or the CLI would silently vanish from the TUI's maps"

## q29 "How are the coding tools switched on?" (model)

- decision d45 "the TUI defaults to code tools only in a git checkout; headless and non-git TUI default to maps; PERCEPT_TOOLS overrides either" (model)
  why: "the code toolset's undo is a git snapshot, so an unconditional default broke the TUI in a .percept-only project - a layout checkout_root supports; resolve_toolset now picks code only where a git checkout exists"
  source 01a07c5e-04b7-7860-9c9c-07b815f05867
  was d44 "the TUI defaults to code tools while headless commands default to maps; PERCEPT_TOOLS overrides either" (model)
  was d30 "PERCEPT_TOOLS=code adds them beside the map tools; the default, maps, is today's behaviour" (model)
- weighed o43 "always on, in every turn" (model)
  why: "every turn, reflect included, would gain the tree and spend its tool calls on it"

## q30 "What are the coding tools named?" (model)

- decision d31 "read_file, write_file, edit_file, list_files, find_files, grep_files, bash" (model)
  why: "verb_noun like read_event and read_map, so a bare read is never confused with them; no sed tool, edit_file covers it"
- weighed o45 "shell, running sh -c" (model)
  why: "more abstract, but models are trained on a tool named bash and write bashisms; the tool now runs bash -c so the name is true"

## q31 "Which tool calls ask the user before they run?" (model)

- decision d32 "write_file, edit_file and bash ask; the rest run; percept ask declines an ask unless --yes" (model)
  why: "reads are routine, mutation and arbitrary commands are not; a headless turn has no one to ask"

## q32 "How many tool calls may a coding turn make?" (model)

- decision d33 "50 with PERCEPT_TOOLS=code; 5 stays for maps" (model)
  why: "a coding task reads several files before one edit; five ends it mid-read"

## q33 "How is a coding turn undone?" (model)

- decision d34 "a commit at refs/percept/snapshots/<prompt id> before each prompt when the coding tools are on; /undo restores the last one" (model)
  why: "scratch refs leave the branch and index alone; version control is the undo the user already knows; the id ties it to the prompt event"
- weighed o44 "an overlay filesystem the shell writes through" (model)
  why: "a shell command runs on the real disk, so the overlay goes stale or must be written out before every command"

## q34 "How does a user stop approving every tool call?" (model)

- decision d35 "a on the approval row runs the call and every later call of that tool this session; y runs once, n declines" (model)
  why: "one answer per tool per session; nothing is committed to the log, so the next session asks again"
- weighed o46 "remember each approved command text" (model)
  why: "a coding turn varies the command every time, so it would ask as often as before"

## q35 "How does a coding turn learn the project's conventions?" (model)

- decision d36 "AGENTS.md at the checkout root goes into the system prompt every round under PERCEPT_TOOLS=code; absent, nothing is sent" (model)
  why: "the coding agent wrote banner comments, an overlong subject and skipped review because it never saw the rules; a chat over the log has no tree to follow them in and pays nothing"
- weighed o47 "read CLAUDE.md" (model)
  why: "one client's file; percept serves any client, and this repo keeps its body in AGENTS.md with CLAUDE.md as an adapter"

## q36 "Where is future work tracked?" (model)

- decision d37 "a tasks map: task and outcome nodes; resolves, blocks and supersedes edges; every task says why" (model)
  why: "resolves is the word decisions already uses, so a reader learns one vocabulary; a task without a why is a todo nobody can weigh"
- weighed o48 "a task kind on the decisions map" (model)
  why: "an open question is a design item; work is a different reasoning operation, what is next and what it waits on, and earns its own map"

## q37 "Where does a done task go in the tasks render?" (model)

- decision d38 "a Done section below the open tasks, each with its outcome" (model)
  why: "findable without a query; the open list stays the part a session reads first"
- weighed o49 "hidden, reached only with --around" (model)
  why: "keeps the render short, but a resolved task is a landmark a reader may still steer by, and stability weighs more than compactness here"

## q38 "How does the map render stay scannable as groups pile up?" (model)

- decision d42 "head every question at ## with a flat contents list; drop the raising-prompt grouping and the short id" (model)
  why: "the raising prompt is usually 'Approved'; its id names nothing and cannot be looked up; first-seen order alone keeps a question from moving, and one ## per question makes the file its own outline"
  source 01a07bf0-1126-7343-9b34-aff73a7c9a38
  was d39 "head each group with its first node's text plus a short prompt id, and open with a ## contents list of every group" (model)
- weighed o50 "keep the date-and-uuid group heading" (model)
  why: "the raising prompt is often 'Approved' or 'agreed. proceed'; the id names nothing a reader can scan for"

## q39 "Are rejected options shown in the decisions render?" (model)

- decision d40 "show options inline as 'weighed:' lines under the question; evidence stays hidden" (model)
  why: "a bare decision reads thin without the alternatives it beat; evidence is still one --around away"
  was d13 "hide options and evidence from the render"

## q40 "How is a map read as Markdown from the CLI?" (model)

- decision d41 "--format json|md on maps show and maps list, default json, md renders store::markdown" (model)
  why: "additive: existing callers unchanged; md is opt-in and shares the renderer that writes .percept/*.md"
- weighed o51 "flip the maps show default to Markdown" (model)
  why: "breaks the code map's documented jq pipelines and every caller that parses the JSONL; churn with no gain for them"

## q41 "How does a long why read in the render?" (model)

- decision d43 "each property on its own line under its node, never appended to the name" (model)
  why: "a decision plus a long why was one unreadable line; the name and the reason are different things and belong on different lines"

## q42 "How should GitHub issues integrate with the tasks and decisions maps?" (model)

note: "Not decided. Four models sketched: A - issue as a link, a task node carries issue:N, one-way push creates/closes, percept stays truth; B - issue as an event source, a webhook publishes issue.opened/closed under source github and a fold derives tasks; C - bidirectional sync; D - a GithubIssues MapRenderer beside MarkdownFiles. Leaning A plus a thin one-way push. Keep decisions percept-native; keep GitHub a strict projection so it is not a third party in the human/agent merge. Full tradeoffs in the 2026-09-07 session."
- open

## q43 "Can the model reach the code map through read_map?" (model)

- decision d46 "read_map serves the code map too, dispatched by map name, offered in every turn" (model)
  why: "the model had one ergonomic tool for decisions and tasks and a bash incantation for code, so it grepped; read_map now dispatches code to the working-tree walk through a MapReader port so store keeps no sideways dep; --since on code is refused as in the CLI"
- weighed o52 "a separate read_code tool beside read_map" (model)
  why: "percept keeps one map-reading tool on purpose (When is read_map offered: one tool fewer to reason about); a second tool splits that"
- weighed o53 "only point the Derived error at percept maps show code" (model)
  why: "leaves the friction that caused the miss: the model still shells out, learns the CLI syntax, and parses JSONL instead of calling a typed tool"

## q44 "Which gpt-5.6 models does the OpenAI catalog offer, and how are they picked?" (model)

- decision d47 "terra and sol join the OpenAI catalog list; luna stays what main and headless build" (model)
  why: "openai.rs already knows the shape of all three (1.05M window, thinking); only OPENAI_MODELS was one entry, so the /models picker never offered terra or sol. OPENAI_MODEL stays luna, the default with no picker."
- weighed o54 "an OPENAI_MODEL env var so headless runs can pick terra or sol" (model)
  why: "each hosted provider already leans on one static model (see Fireworks); a headless run has no picker, and one fixed default keeps startup predictable"
- weighed o55 "a per-model reasoning-effort table for the gpt-5.6 family" (model)
  why: "no per-model effort mechanism exists; effort stays one global knob and all three share the 1.05M window, so global low is a safe floor"

## q45 "How are OpenAI reasoning-effort levels exposed in the catalog?" (model)

- decision d48 "ModelDescriptor carries low, medium, and high effort capabilities for OpenAI models" (model)
  why: "the catalog tells callers which effort levels a listed model supports; the configured global effort remains what build sends"

## q46 "How does a reader learn a map's node and edge kinds?" (model)

- decision d49 "each kind carries a gloss on its Schema, shown by maps list --format md and the read_map response" (model)
  why: "the gloss lives once beside the kind; both agent surfaces render it; AGENTS.md stops carrying a list that drifts"
- weighed o56 "a maps describe <map> subcommand" (model)
  why: "maps list --format md already prints one section per map; a third verb to show what a second verb can"
- weighed o57 "keep the kind list in AGENTS.md prose" (model)
  why: "it duplicated code.md, drifted, and this session's model read package:providers by the wrong meaning"

## q47 "What does maps list --format md render per map?" (model)

- decision d50 "a per-map section: purpose, counts, kinds with glosses, and one live example node and edge line" (model)
  why: "the section holds what a table cannot; the example anchors the kind names to real strings"
- weighed o58 "keep the single table, add columns for kinds" (model)
  why: "a table cell cannot hold a kind-and-gloss list plus a JSONL example line"

## q48 "Does AGENTS.md list the code map's node kinds?" (model)

- decision d51 "no; the code row points at percept maps list, and the preamble notes code keys internal code by file path" (model)
  why: "the Package column trained the model to try package:providers this session; the kinds now live on the Schema"

## q49 "How does the TUI select reasoning effort?" (model)

why: "The catalog exposes supported effort levels, but the user needs a lightweight way to change the live session setting."
- decision d52 "a session-only /effort command with contextual suggestions and Tab completion" (model)
  why: "It uses normal command syntax and the existing suggestion area; Enter applies a complete value without starting a model turn, while bare /effort reports current and available values."

## q50 "What happens to reasoning effort when the model changes?" (model)

why: "Different models can support different effort levels."
- decision d53 "keep the selected effort when supported; otherwise use the new model's configured default" (model)
  why: "A compatible selection remains stable, while an incompatible one cannot leak into requests for the new model."

## q51 "Are the maps and code toolsets two harnesses?" (model)

- decision d54 "one harness, with the tree on or off; PERCEPT_TOOLS and PERCEPT_MAPS stay as knobs on it" (model)
  why: "a harness is for a purpose; the toolsets differ by whether a git checkout exists, which is the environment, so there is nothing to pick and no variable to add"
- weighed o59 "maps and code as named harnesses picked by PERCEPT_HARNESS" (model)
  why: "two settings of one thing; a variable to select between them names a difference that is not a purpose"

## q52 "How far back does the model's history reach?" (model)

- decision d55 "a token window: one eighth of the model's context, cut back to one sixteenth, 8k floor when the model reports none; results outside the current turn as preview lines" (model)
  why: "twenty raw events lost a confirmed plan before the build turn on a 1M model; a share scales with the model, and the cut lands in steps so the request's prefix stays cacheable between cuts"
- weighed o60 "raise CONTEXT_EVENTS" (model)
  why: "fights the design that says a model searches what it cannot hold, and 40 events still miss a plan written before a six-exchange discussion"
- weighed o61 "record the confirmed plan as a task node so the tasks map carries it" (model)
  why: "a specific map is content, not the harness; the builder must work for any map, so the fix belongs in how the window is cut"

## q53 "What does the model see past the window?" (model)

- decision d56 "one preview line per event with its id, back 200 events, so read_event can open any of them" (model)
  why: "the log as environment with handles into it; percept ranks nothing, the line is the same constant-size output search_events gives"

## q54 "In what order does a request carry its sections?" (model)

- decision d57 "stable first: instructions, maps, index, history, then the time last" (model)
  why: "every provider reuses a matching prefix; the time at position one changed every round and killed the cache, so a fifty-call turn paid for the maps fifty times"

## q55 "How does a user see what the model was shown?" (model)

- decision d58 "/context in the TUI: each section's shape and estimated tokens beside the last round's input and cached tokens" (model)
  why: "the two claims of the window work, that it reached the plan and that the cache held, cannot be checked without it; both coding clients call it /context"

## q56 "Where do design proposals live?" (model)

- decision d59 "docs/, tracked; the /docs ignore line is dropped" (model)
  why: "the harness design is the first; a tracked folder beside AGENTS.md is where a reader looks"

## q57 "How does the prompt stay within the model's window as the maps grow?" (model)

note: "Not decided. The maps are always in view and never shrink, so the prompt grows without bound; on a 16k model the instructions and the decisions map exceed the window before history is counted and the provider truncates the instructions. Options: apply the precedence rule in docs/harness.md, dropping the maps to headlines and then to a catalogue line when over budget; a token budget per map; keep superseded decisions out of the prompt shape while the render keeps them. Task: let the instructions and maps give way under the budget on a small window."
- open

## q58 "How does a rendered map stay scoped to the branch it is committed in?" (model)

note: "Not decided. One log holds every branch, and the render is a file in one branch, so recording from any worktree rewrites the render with unmerged branches' nodes; hit in four sessions, again on feat/harness with two questions from the effort branch. Options: stop committing the render and regenerate on demand; regenerate in a pre-commit step; teach the renderer a branch scope from the events' sources or the checkout. Task: stop committing the rendered map Markdown, or scope its rewrite to the branch."
- decision d75 "the rendered map Markdown is dropped: no MarkdownFiles renderer, nothing written to .percept beyond the hand-written index.md, and the three files already tracked are removed from git" (model)
  why: "a render is a file in one branch; recording from any worktree rewrote it with that branch's unmerged nodes. Dropping the file removes what there is to scope, rather than teaching the renderer a branch. A map is read live via percept maps show --format md and the session-start fragment instead"
  source 01a08625-4818-77a3-957a-f01c2ef0bd37

## q59 "Where does agreement between the human and the agent live?" (model)

- decision d62 "in the core, as confirms and disputes edges from a human contour to a node another contour wrote, and a standing the fold derives per confirmer: claimed, confirmed by whom, disputed by whom; contours are many, of two kinds" (model)
  why: "several agents write to one log and a subagent is a contour of its own, so two was never the count; the transparent-or-opaque distinction is by kind, not number, and the mechanism is unchanged; standing per confirmer is the same fold for one person or ten, and writing two into the edge's definition would be undone when the confirms edge is built; docs/architecture.md"
  source 01a0810c-d677-7680-909f-98bcca5b4e9a
  was d60 "in the core, as confirms and disputes edges from a user to a model node and a standing the fold derives: claimed, confirmed, disputed; every schema carries it" (model)
- weighed o62 "one map per cognition, merged at a shared surface" (model)
  why: "doubles what a reader holds for a distinction the fold derives from the actor and the confirmation edge; the contour is a view over one map, never a storage boundary"
- weighed o65 "agent-to-agent confirmation counts as agreement" (model)
  why: "an agent confirming another agent's node is a second claim from inside the log, not agreement; the rule exists for the cognition whose head is outside it"

## q60 "What is percept's core called?" (model)

- decision d61 "cognitive rails: constraints on what cognition may write to its record and read from it, defined at first use in AGENTS.md" (model)
  why: "the doc argued against cognitive architecture and then used it; SOAR and ACT-R prescribe how thinking proceeds and percept does not; a term disclaimed at first use is the wrong term; the definition says rails on the record, not a guardrail on what the model says, since rails means safety filters in LLM tooling"
- weighed o63 "cognitive architecture" (model)
  why: "names a processing loop in SOAR and ACT-R; a newcomer knows the term, but it had to be disclaimed at first use"
- weighed o64 "cognition rails" (model)
  why: "more accurate, since the rails are not themselves cognitive, but not idiomatic; the repo already says cognitive map, commit and history in the same loose sense"

## q61 "How is a human contour identified?" (model)

note: "Not decided. The core has an actor kind for the human and no identity behind it; standing per confirmer needs one only when a second person confirms. Waits for that person, and for the confirms edge to exist. Options: an id on the user actor; the source name, as agents have; a hosted log with accounts."
- open

## q62 "Where does a map's schema live?" (model)

- decision d66 ".percept/schemas/<name>.toml in the checkout; decisions and tasks ship embedded as the same TOML, and a project file of the same name extends one - every built-in kind, headline, and settlement kept, kinds added - never shrinks it" (model)
  why: "the correctness review showed a project decisions.toml that drops a kind the log holds breaks every fold at startup, and the render and the never-remove rule assume the built-in kinds; extend-only keeps what a project wants from an override - purpose, glosses, an extra kind - without those failures"
  source 01a0814f-a1cf-7041-a2fa-520b44e7bdf7
  was d63 ".percept/schemas/<name>.toml in the checkout; decisions and tasks ship embedded as the same TOML, and a project file of the same name overrides" (model)
- weighed o66 "a map.declared event in the log, recorded by a percept maps declare verb" (model)
  why: "a schema has no provenance to cite; changing one later needs a supersede protocol for schemas; the verb is a step a file edit removes"
- weighed o67 "Rust consts, as today" (model)
  why: "a session that wants a glossary map needs a Rust change"

## q63 "What format is a schema file?" (model)

- decision d64 "TOML: name, purpose, headlines, settles, then [[nodes]] and [[edges]] array tables, each with a gloss and an optional requires list" (model)
  why: "every Rust dev and coding agent writes Cargo.toml; comments; array tables keep schema order, which the render sections and maps list use; the toml crate is maintained"
- weighed o68 "YAML" (model)
  why: "serde_yaml is archived and unmaintained; indentation errors and the no-becomes-false class of surprises"
- weighed o69 "JSON, the log's own format" (model)
  why: "no comments and every key quoted: hostile to the hand-editing a file exists for"
- weighed o70 "KDL" (model)
  why: "a node document fits nodes and edges, but few readers know it and an agent writes it worse than TOML"

## q64 "How does a schema say which properties a node must carry?" (model)

- decision d65 "a requires list on the kind; option and task require why" (model)
  why: "one place beside the gloss, and the file carries it; revise checks any kind the same way, replacing the two checks that compared the schema to DECISIONS and TASKS"
- weighed o71 "check requires in mapstore's write path, kept out of Map::apply so old history still folds" (model)
  why: "built first on feat/schemas-as-data and moved: a fold calls replay, never apply, so Map::apply is only ever the write path and the rule belongs there; kept outside core it reached the CLI but not the model's revise_map tool, and AGENTS.md says the rules live once in Map::apply"

## q65 "How does the hook command learn which event fired?" (model)

- decision d67 "percept hook <client>: the event name is read from the input's hook_event_name, no event argument" (model)
  why: "both clients put the event name in the input; an argument would be a second source of one fact"
- weighed o72 "percept hook <client> <event>, as docs/clients.md first wrote it" (model)
  why: "the input already carries the event; a config line per event that must match it is a place to drift"

## q66 "How does a hook config line find the binary?" (model)

- decision d68 "percept on PATH; PERCEPT_BIN is retired" (model)
  why: "install.sh puts the binary on PATH and the worktree recipe in the README already exports one, so one way to find the binary is enough"
- weighed o73 "keep PERCEPT_BIN: \"${PERCEPT_BIN:-percept}\" hook <client> in the config line" (model)
  why: "a second way to find one binary, and a config line with shell expansion in it that every client must run through a shell"

## q67 "Which percept commands does init allowlist for Claude Code?" (model)

- decision d69 "Bash(percept maps *) and Bash(percept events *) in permissions.allow; Codex gets hooks only, it has no per-command allowlist in this shape" (model)
  why: "recording a decision costs three permission prompts without it; maps and events only read the log or append to it"
- weighed o74 "Bash(percept *)" (model)
  why: "ask --yes runs tools without asking, so an allowlisted line would let the model bypass write approval"

## q68 "Where does the hook's turn state live?" (model)

- decision d70 "beside the log, at <log dir>/hook-sessions" (model)
  why: "a dev build keeps it in the checkout with its log; one rule says where percept's local data goes"
- weighed o75 "$PERCEPT_HOME/hook-sessions, as the Python hook kept it" (model)
  why: "the script could not ask the binary where the log was, so it kept its own default; the binary knows"
- weighed o78 "a hash of client, root, session and turn as the state file's name" (model)
  why: "std's DefaultHasher is unspecified across toolchains, so two builds mid-turn disagree and orphan the file; a readable path under hook-sessions/<root>/<client>-<session>-<turn> needs no hash; built and replaced 2026-09-08"

## q69 "Where does percept init write a client's config?" (model)

- decision d71 "the tracked project file, .claude/settings.json or .codex/hooks.json, merged with what is there; a second run changes nothing" (model)
  why: "the config is the repo's, shared by everyone who opens it; merging keeps a project's other settings"
- weighed o76 ".claude/settings.local.json" (model)
  why: "local is one person's overrides; a hook every session in the repo depends on is not one person's"

## q70 "How does the hook find the checkout?" (model)

- decision d72 "from the hook input's cwd, by the binary's own root walk" (model)
  why: "a project with only .percept records too, and the rule for what a project is lives once"
- weighed o77 "git rev-parse --show-toplevel, as the Python hook did" (model)
  why: "needs git on the path and records nothing in a .percept-only project"

## q71 "When does the shared log outgrow a full-file fold, and what fixes it?" (model)

note: "Not decided. Log growing ~15MB/3700 events per day (60MB/14866 events over 4 days, 2026-09-05 to 2026-09-09); at that pace ~5GB/1.3M events a year. A fold today (log.load + Map::fold, one project) already reads and discards every other project's events too, since EventLog::load has no way to skip past what Scope will reject - cost scales with total activity across every project on the machine, not the one queried. Fold itself is fast now (67ms at 60MB, ~900MB/s parse). SQLite would help by indexing source.path so a query skips straight to one project's rows, but drops the one-file/jq-pipeable property events search and show rely on, and needs a migration. A cheaper first move: a sidecar index (project path to byte ranges) that load() consults, kept incremental on append, no format change. Revisit once the log crosses roughly 500MB-1GB; not a problem at today's size."
- open

## q72 "How is a short id minted and displayed for a node?" (model)

- decision d73 "a per-kind sequence number minted once at creation and stored on the node; the display prefix is an optional schema field, defaulting to the kind name's first letter" (model)
  why: "the number is what gets cited in chat, PR comments, and committed text, and must never change; the prefix is cosmetic and a project may rename it without a Rust change or a migration, since resolution reads the schema fresh each time. Minting is a local per-project counter, not merge-safe against independent offline writers - that is left to whenever a distributed-log merge design is actually built, tied to the still-open \"How is a human contour identified?\" question"
- weighed o79 "ids computed fresh from fold order at display time, never stored" (model)
  why: "Scope changes fold order - a project-scoped and an --all-projects show fold different subsets, so the same node gets a different ordinal in each view; not stable enough to cite in chat or commit to a file"
- weighed o80 "a short hash of the node's uuid" (model)
  why: "unmemorable, and collision-prone at a length short enough to stay readable"
- weighed o81 "a hardcoded kind-to-letter table in Rust" (model)
  why: "a project could not rename or add a prefix without a Rust change, unlike gloss and requires which already live in the schema"

## q73 "How does the model learn what changed since it last opened this project?" (model)

- decision d74 "a session.started event per source and project; SessionStart finds the previous one to compute since" (model)
  why: "everything else in the log is an event already; a side file needs its own repair and locking story that EventLog::append already solves, and \"since\" is exactly what Selection::since already implements"
