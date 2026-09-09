# percept

An experience log and shared cognitive maps. See [AGENTS.md](AGENTS.md)
for the architecture and [.percept/index.md](.percept/index.md) for the
maps.

## Reading decisions

`percept maps show decisions --format md` lists questions in the order
they were raised, each with the decision that settles it now. Options,
evidence, and superseded decisions stay in the map and are one query
away:

```sh
percept maps list
percept maps show decisions --around 'question:Where does the event log live?'
percept maps show decisions --since 1d
```

A filtered read reports on stderr how much of the map it left out;
stdout stays JSONL. The model's `read_map` tool takes the same cut and
returns the same counts. A fragment is where checking starts, not proof
that nothing else bears on it.

`PERCEPT_MAPS` says how much of each map reaches the model each turn:
`prompt` (the default), `headlines`, or `tool`. In every shape the
prompt carries one line per map with its purpose, size, and last
change, and `read_map` is offered, so the model can cut a map around
one node even when the whole map is in the prompt. The code walk is
not a map: it reaches the model only as `read_code`, under the code
toolset.

## Coding agents

The setup is client-neutral: one body of instructions, skills, and
event capture, and per client only the files that point at it. Claude
Code and Codex are wired today.

| Shared source | Claude Code entry | Codex entry |
|---|---|---|
| `AGENTS.md` | `CLAUDE.md` imports it | Loaded directly |
| `.agents/skills/` | `.claude/skills/` symlinks | Discovered directly |
| `.agents/agents/software-developer.md` | `.claude/agents/software-developer.md` | `.codex/agents/software-developer.toml` |
| `percept hook <client>` | `.claude/settings.json` | `.codex/hooks.json` |

To add a client, point its skill discovery at `.agents/skills` and its
hooks at `percept hook <client-name>`. The binary records every event
under that name as its source, so `percept events search --source
<client-name>` reads one client's history. It reads the hook input
shape Claude Code and Codex share; a client that sends another shape
needs a small translation in `src/cli/hook.rs`, not a hook of its own.

Install the binary with `scripts/install.sh`, which puts `percept` on
PATH; the hooks find it there. `percept init claude-code` or `percept
init codex` writes the client's three hook entries into the checkout,
and for Claude Code also allows `percept maps` and `percept events`
without a permission prompt. Both files are committed here already.
Open the client from this checkout and trust the repository. In Codex,
use `/hooks` to review and trust the three capture hooks. Restart an
existing client session to load the project configuration.
See the official [Codex hooks](https://learn.chatgpt.com/docs/hooks) and
[skills](https://learn.chatgpt.com/docs/build-skills) documentation.

Older checkouts may have percept hooks in `.claude/settings.local.json`
and an untracked `.claude/skills/percept/` directory holding
`on-prompt.sh`, `on-tool.sh`, and `on-stop.sh`. Remove those hook
entries and that directory before using the versioned configuration,
or every prompt is recorded twice and the shared skill symlink cannot
be created. Keep unrelated local settings.

The hooks capture user prompts, completed tool calls and results, and
the final reply. A prompt hook returns its event ID for later citations.
Each client keeps its own source name, `claude-code` or `codex`.
A capture error is printed to stderr and the hook exits non-zero, which
is how the client shows it; the turn continues. Events before hooks
were enabled are not imported automatically.

## Comparing worktrees

Percept treats worktrees as the same project. To keep experiments out
of the shared log, launch the client with a separate state directory:

```sh
cargo build --offline
export PERCEPT_HOME="$PWD/.percept"
export PATH="$PWD/target/debug:$PATH"
codex
# Or:
claude
```

The hooks run whichever `percept` is first on PATH, so the example
uses this branch's binary independently of the state directory. Session
causation is isolated by client, checkout, session, and turn where the
client supplies it.

The log and hook state are local data, not files to commit. A worktree
without historical events starts with an empty live map, even when a
tracked Markdown view exists. An empty map does not mean no prior
decision.

## Verification

```sh
cargo build --offline
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
python3 -m unittest discover -s scripts -p 'test_*.py'
```
