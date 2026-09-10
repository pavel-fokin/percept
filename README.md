# percept

percept keeps project decisions alive across AI coding sessions.

It records what happens across your tools - prompts, replies,
tool calls, files read - as an append-only log of events. A model
searches that log and writes what it concludes into maps: a decisions
map, a tasks map, an ideas map, or one you define. Every change to a
map is itself an event in the same log, citing the experience it was
drawn from. A map can be rebuilt from its history, and every claim in
it can be checked against what was actually seen.

It serves Claude Code and Codex through hooks and exposes the log and
maps on the command line.

- [AGENTS.md](AGENTS.md) - the design, the domain, and the architecture.
- [.percept/index.md](.percept/index.md) - the directory of this repo's maps.
- [docs](docs) - proposals and related work.

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

Requires a Rust toolchain.

```sh
git clone https://github.com/pavel-fokin/percept
cd percept
make install
```

This builds a release binary and copies it to `~/.percept/bin`, beside
the log. If that directory is not on `PATH`, the script puts a copy in
`~/.local/bin` or `~/bin`, and otherwise prints the line to add. Rerun
after every change: the hooks run whichever `percept` is first on
`PATH`.

## Quick start

Record a Claude Code session and read what it left behind:

```sh
cd your-project
percept init claude-code       # writes .claude/settings.json hooks
claude                         # work as usual; prompts and replies are recorded

percept events search --since 1h
percept maps show decisions --format md
```

`percept init codex` does the same for Codex. The [Coding
clients](#coding-clients) section says what the hooks capture.

## The log

One file, `~/.percept/percept.jsonl`, holds every project. Each event
names its source - the writer and the project root it ran in - and a
read picks the current project by default, or every one with
`--all-projects`. Each line also carries the log's own id, kept in
`log-id` beside the file, and its number in that log, so two logs can
be merged later.

```sh
percept events search --since 1d --type message.received
percept events search --contains worktree --size 5
percept events search --source codex
percept events show <id>
percept events show <id> --range 400:
percept events publish --actor human --source percept-cli \
  --type message.received --payload '{"content":"..."}'
```

Output is one JSON object per line, oldest first. A search line keeps
a constant size, so looking is cheap; `--full`, `show`, or `show
--range` spends tokens on one event deliberately. percept never ranks,
summarises, or answers. Relevance is the caller's judgement.

## Maps

A map is folded live from the log on every read. Nothing is rendered
to a file.

```sh
percept maps list --format md
percept maps show decisions --format md
percept maps show decisions --around 'question:Where does the event log live?'
percept maps show decisions --since 1d
percept maps show tasks --since 1d
```

The Markdown render of the decisions map lists each question with the
decision that settles it now. Options, evidence, and superseded
decisions stay one hop away through `--around`. A cut reports on
stderr how much of the map it left out.

Writes go through the same binary. Every one cites the events it was
drawn from and says who is writing, `user` or `model`:

```sh
percept maps add-node decisions --actor agent --kind decision --name "..." \
  --prop why="..." --source <event-id>
percept maps add-edge decisions --actor agent --kind resolves --from d42 --to q7

percept maps record decisions --actor agent --source <event-id> <<'EOF'
question "Where does the log live?"
decision "one log under ~/.percept"
  why "one variable also covers the binary"
  resolves question
  cites src/main.rs:40-52
EOF
```

`record` takes a whole document: a node per line at the margin, with
its properties, edges, and `cites` lines indented under it. A `cites`
line publishes the named text as a `file.cited` event, so a later
session can see whether the file still says what the claim rested on.

A model's node carries a standing - `claimed`, `seen`, `confirmed`, or
`disputed` - the fold derives from the human's own judgment, never from
an edge:

```sh
percept maps confirm decisions d41
percept maps dispute decisions d41 --why "never proposed"
```

`confirm` marks a node's claim right; `dispute` marks it wrong, with
why. Both refuse a node the map does not hold and the human's own node,
since a user-written node carries no standing to judge.

A map's schema is a TOML file at `.percept/schemas/<name>.toml` naming
its node and edge kinds and one line of purpose. `decisions` and
`tasks` ship built in; this repo adds `ideas`. The rules for a map -
who may remove what, how a decision is corrected - are in
[AGENTS.md](AGENTS.md). How to read, check, and revise one is in the
[percept skill](.agents/skills/percept/SKILL.md).

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

`percept init <client>` writes the client's hook entries into the
checkout, merging into an existing file. For Claude Code it also
allows `percept maps` and `percept events` without a permission
prompt. Both files are committed in this repo. Open the client from
the checkout and trust the repository; in Codex, `/hooks` reviews the
capture hooks. Restart a running session to load the configuration.

The hooks call `percept hook <client>` on session start, each prompt,
and each reply. `percept init <client> --capture` adds each completed
tool call and its result, which fills the log with every file the
agent read; this repo's own configs carry it, a project that only
wants its decisions does not. Session start prints a bounded fragment
of every map into the client's context: what changed since the last
session, what you confirmed or disputed since then with your words,
what is open, and whether any cited file has changed, then the
recording rules. The other hooks append events under the client's
name. A
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
cargo run --features lab -- reflect       # one turn revising the decisions map
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
| `PERCEPT_MAPS` | `prompt`, `headlines`, `tool` | `prompt` |

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
