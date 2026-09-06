# percept

An experience log and shared cognitive maps. See [AGENTS.md](AGENTS.md)
for the architecture and [.percept/index.md](.percept/index.md) for maps.

## Reading decisions

The decisions overview shows lasting commitments. Expand one to inspect
its supporting choices, rationale, and source events. Grouping preserves
the original nodes and relationships. Unassigned material remains visible.

```sh
percept maps list
percept maps show decisions --kind commitment
percept maps show decisions --around 'commitment:Event storage' --depth 2
percept maps show decisions --markdown
```

Filtered CLI reads report omitted material on stderr; stdout stays JSONL.
Markdown exports carry the coverage notice in the document itself.
The model's `read_map` tool includes the same counts with its result.
A fragment is a starting point for verification, not proof of completeness.

`ask` and the TUI default to `PERCEPT_MAPS=tool`: map purposes and sizes
reach the prompt, and the model selects what to open. `headlines` and
`prompt` remain available as explicit settings. The code map stays out
of model prompts.

## Claude Code and Codex

Both clients use the same repository instructions, skills, and event
capture. Client files contain only the discovery metadata and commands.

| Shared source | Claude Code entry | Codex entry |
|---|---|---|
| `AGENTS.md` | `CLAUDE.md` imports it | Loaded directly |
| `.agents/skills/` | `.claude/skills/` symlinks | Discovered directly |
| `.agents/agents/software-developer.md` | `.claude/agents/software-developer.md` | `.codex/agents/software-developer.toml` |
| `scripts/agent-hook.py` | `.claude/settings.json` | `.codex/hooks.json` |

Install the binary with `scripts/install.sh`. Hooks need Python 3 and
Git. Open either client from this checkout and trust the repository.
In Codex, use `/hooks` to review and trust the three capture hooks.
Restart an existing client session to load the project configuration.
See the official [Codex hooks](https://learn.chatgpt.com/docs/hooks) and
[skills](https://learn.chatgpt.com/docs/build-skills) documentation.

Older checkouts may have percept hooks in `.claude/settings.local.json`.
Remove only entries invoking `.claude/skills/percept/on-prompt.sh`,
`on-tool.sh`, or `on-stop.sh` before using the versioned configuration.
Keep unrelated local settings. This worktree has no legacy local file.

The hooks capture user prompts, completed tool calls and results, and
the final reply. A prompt hook returns its event ID for later citations.
Each client keeps its own source name, `claude-code` or `codex`.
Capture errors report to stderr and let the coding session continue.
A missing binary disables capture. Events before hooks were enabled
are not imported automatically.
Payloads exceeding the operating system's argument limit cannot be
captured through the current publish CLI; the hook reports that error.

## Comparing worktrees

Percept treats worktrees as the same project. To keep experiments out
of the shared log, launch the client with a separate state directory:

```sh
cargo build --offline
export PERCEPT_HOME="$PWD/.percept"
export PERCEPT_BIN="$PWD/target/debug/percept"
export PATH="$PWD/target/debug:$PATH"
codex
# Or:
claude
```

Without `PERCEPT_BIN`, hooks use `~/.percept/bin/percept`. The example
uses this branch's binary independently of the state directory. Session causation is
isolated by client, checkout, session, and turn where the client supplies it.

The log and hook state are local data, not files to commit. A worktree
without historical events starts with an empty live map, even when a
tracked Markdown view exists. An empty map does not mean no prior decision.

## Verification

```sh
cargo build --offline
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```
