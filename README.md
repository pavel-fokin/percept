# percept

percept keeps project decisions alive across AI coding sessions.

It records what happens across your tools - prompts, replies,
tool calls, files read - as an append-only log of events. A model
searches that log and writes what it concludes into maps: a decisions
map, or one you define. Every change to a
map is itself an event in the same log, citing the experience it was
drawn from. A map can be rebuilt from its history, and every claim in
it can be checked against what was actually seen.

It serves Claude Code and Codex through hooks and exposes the log and
maps on the command line.

- [AGENTS.md](AGENTS.md) - the design, the domain, and the architecture.

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [The log](#the-log)
- [Maps](#maps)
- [Coding clients](#coding-clients)
- [Configuration](#configuration)
- [Development](#development)
- [The lab](#the-lab)

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/pavel-fokin/percept/main/scripts/get.sh | sh
```

Downloads the newest binary for your platform (macOS or Linux, Intel
or ARM) and copies it to `~/.percept/bin`, beside the log. If that
directory is not on `PATH`, the script symlinks it from `~/.local/bin`
or `~/bin`, and otherwise prints the line to add.

### Building from source

Requires a Rust toolchain.

```sh
git clone https://github.com/pavel-fokin/percept
cd percept
make install
```

Installs the same way as `get.sh`. Rerun after every change: the
hooks run whichever `percept` is first on `PATH`.

## Quick start

Record a Claude Code session and read what it left behind:

```sh
cd your-project
percept init claude-code       # writes .percept/schemas and .claude/settings.json hooks
claude                         # work as usual; prompts and replies are recorded

percept search --since 1h
percept show
```

`percept init codex` does the same for Codex. The [Coding
clients](#coding-clients) section says what the hooks capture.

## The log

One file, `~/.percept/percept.jsonl`, holds every project. Each event
names its source - the writer and the project root it ran in. Each
line also carries the log's own id, kept in `log-id` beside the file,
and its number in that log, so two logs can be merged later.

```sh
percept search --since 1d --type message.received
percept search worktree --size 5
percept search --source codex
percept show <event-id>
percept show <event-id> --range 400:
```

`search` reads the level it runs in: inside a project, that project's
events; in `~`, outside any project, the whole log. Output is one JSON
object per line, oldest first. A search line keeps a constant size, so
looking is cheap; `--full`, `show`, or `show --range` spends tokens on
one event deliberately. percept never ranks, summarises, or answers.
Relevance is the caller's judgement.

## Maps

A map is folded live from the log on every read. Nothing is rendered
to a file.

```sh
percept show                   # every map, one line each
percept show concepts          # one map whole
percept show c3                # a node and its neighbours
percept show concepts --since 1d
```

Writes name no map. The kind says which map a node belongs to, and an
edge goes to the map of its two nodes. Every write says who is writing
and can cite the events it was drawn from:

```sh
percept add concept "Snapshot" --definition "the working tree saved under a prompt" \
  --actor agent --source <event-id>
percept add covers c1 c3
percept change c3 --definition "...; undo puts it back"
percept remove covers c1 c3

percept add --actor agent --source <event-id> <<'EOF'
concept "Snapshot"
  definition "the working tree saved under a prompt"
  covers c1
  cites src/main.rs:40-52
EOF
```

A write prints the node's short id, its map, and the level it landed
on: `c3  concepts (project)`. `add` with no kind takes a whole document:
a node per line at the margin, with its properties, edges, and `cites`
lines indented under it. A `cites` line publishes the named text as a
`file.cited` event, so a later session can see whether the file still
says what the claim rested on.

A map's schema is a TOML file at `.percept/schemas/<name>.toml` naming
its node and edge kinds and one line of purpose. Schemas live at two
levels: `~/.percept/schemas` declares global ones, whose maps every
project shares, and a project's own `.percept/schemas` declares its
own. A node kind and its short id prefix belong to one schema across
both. `percept init` writes the shipped schemas into the project, and
a global `projects` schema into `~/.percept/schemas` if none is there;
a project with no schema files at either level has no maps.

## Coding clients

Claude Code and Codex record into the log through hooks. The setup is
client-neutral: one body of instructions, skills, and event capture,
and per client only the files that point at it.

| Shared source | Claude Code entry | Codex entry |
|---|---|---|
| `AGENTS.md` | `CLAUDE.md` imports it | Loaded directly |
| `.agents/skills/` | `.claude/skills/` symlinks | Discovered directly |
| `.agents/agents/software-developer.md` | `.claude/agents/software-developer.md` | `.codex/agents/software-developer.toml` |
| `percept hook <client>` | `.claude/settings.json` | `.codex/hooks.json` |

`percept init <client>` writes the shipped schemas under
`.percept/schemas`, leaving a file already there alone, and the
client's hook entries into the checkout, merging into an existing
file. For Claude Code it also allows `percept add`, `remove`,
`change`, `show`, and `search` without a permission prompt. Both files are committed in this repo. Open the client from
the checkout and trust the repository; in Codex, `/hooks` reviews the
capture hooks. Restart a running session to load the configuration.

The hooks call `percept hook <client>` on session start, each prompt,
each completed tool call with its result, and each reply, so the log
holds what the agent read and ran beside what was said: a map that
cites only the conversation cites no experience. Session start prints what `percept
start` prints from the shell: what each map holds, what moved since
the last session and which cited files changed, and the commands to
go next. The other hooks append events under the client's name. A
capture error goes to stderr and the hook exits non-zero, which is how
the client shows it; the turn continues.

To add a client, point its skill discovery at `.agents/skills` and its
hooks at `percept hook <client-name>`. The binary reads the hook input
shape Claude Code and Codex share. A client that sends another shape
needs a translation in `src/cli/hook.rs`, not a hook of its own.

## Configuration

| Variable | Values | Default |
|---|---|---|
| `PERCEPT_HOME` | state directory: the log, hook state, the binary | `~/.percept` |

A binary run from `target/` ignores `~/.percept` and keeps its state
under the checkout's own `.percept/`, so working on percept does not
mix test events into the shared log. `PERCEPT_HOME` overrides both.

## Development

```sh
cargo build --offline
cargo test --offline
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline --all-features
cargo clippy --offline --all-features --all-targets -- -D warnings
```

The default build is the binary above. `--all-features` adds the lab,
so a change to the core that breaks it fails here and not later. Run
both: below the lab, `core`, `store`, `mapstore`, and `workspace` are
judged with the lab present, and the default clippy is what catches a
lab symbol reaching `cli` or `main` ungated.

Worktrees are the same project as far as the log is concerned. To keep
an experiment's events apart, run it with its own `PERCEPT_HOME`. A
checkout with no events for a map folds an empty one; that is not
evidence that no decision was made.

The workflow for changes - plan, build, review, reflect - is in
[AGENTS.md](AGENTS.md).

## The lab

percept's own coding agent is a lab for one claim, the log as an
environment the model searches instead of a transcript it reads. It is
not part of what a developer installs: it builds only under the `lab`
feature, and `scripts/install.sh` leaves it out.

```sh
cargo run --features lab                  # the TUI
cargo run --features lab -- ask "what did the last session leave open?"
cargo run --features lab -- ask --yes "rename Foo to Bar"    # run calls the policy would ask about
```

The TUI is a chat over the log with tools, and in a git checkout a
coding agent over the working tree. When a tool call needs approval
the turn pauses on its row: `y` runs it once, `a` runs it and allows
that tool for the session, `n` declines. `Esc` quits.

| Command | Does |
|---|---|
| `/models` | switch the model |
| `/undo` | put the working tree back as it was before the last turn |
| `/context` | show what the model was sent |
| `/effort` | set the model's reasoning effort for the session |

| Variable | Values | Default |
|---|---|---|
| `PERCEPT_PROVIDER` | `ollama`, `openai`, `fireworks` | `ollama` |
| `OPENAI_API_KEY`, `FIREWORKS_API_KEY` | the provider's key | |
| `PERCEPT_TOOLS` | `code`, `maps` | `code` in a git checkout, `maps` elsewhere and headless |
| `PERCEPT_MAPS` | `prompt`, `overview`, `tool` | `prompt` |

Ollama is expected at `localhost:11434`.

`PERCEPT_TOOLS=code` adds the file tools, `bash`, and `read_code`, a
walk of the checkout's files, symbols, and imports. It asks before a
write and snapshots the tree before each prompt. `PERCEPT_MAPS` says
how much of each map the prompt carries; in every shape the model can
cut a map around one node with `read_map`.

The TUI needs a real terminal. `scripts/drive.py` forks a pty over
the lab build, sends
timed keystrokes, and prints the frames; `--plain` strips the escapes
so the text can be grepped.
