# Querying the code map

`percept maps show code` walks the working directory and prints a graph
of the Rust code: one JSON line per node, then one per edge. It reads
the tree fresh each time, so it is never stale. It prints JSONL on
stdout and selection notices on stderr. Compose stdout with `jq`,
`grep`, and shell loops. It
never ranks or summarises - you decide what is relevant.

In a turn with the `read_map` tool, call it with `map` set to `code`
for the same graph and the same `around` / `depth` / `kinds` cuts, no
shell needed. `since` has no meaning for `code` - it has no history.

Binary: `~/.percept/bin/percept`, installed by `scripts/install.sh`.

Node kinds are `file`, `function`, `type`, and `package`; edge kinds
`contains` and `imports`. `percept maps list --format md` glosses each
and shows one example line - read it once rather than guessing what a
kind holds.

Beyond the name: a `file` carries a `language` property; a `function`
or `type` carries `public` (`"true"`/`"false"`) and `line`. A
`function` name is path-qualified - `src/percept/map.rs::map_of` for a
free function, `src/percept/map.rs::Map::apply` for a method,
`Event::From<&percept::Event>::from` for a trait impl method, keeping
the trait's arguments.

An edge line names its ends as `kind:name`, so it reads on its own:

    {"edge":"imports","from":"file:src/main.rs","to":"package:clap","sources":[]}

## Look at a neighbourhood, not the whole map

The whole map is thousands of lines. Cut it first:

- `--around kind:name` keeps one node and what is within `--depth`
  edges of it (default 1), in either direction.
- `--kind K` keeps only nodes of kind `K` and the edges between them.
  Repeatable.

What one file imports and what imports it:

    percept maps show code --around file:src/store/mod.rs --kind file \
      | jq -r 'select(.kind) | .name'

Every symbol a file defines, with visibility:

    percept maps show code --around file:src/percept/map.rs --kind function --kind type \
      | jq -r 'select(.kind) | "\(.kind) \(.name) pub=\(.properties.public) line=\(.properties.line)"'

Which files depend on a file - the edges pointing at it:

    percept maps show code --kind file \
      | jq -r 'select(.to=="file:src/percept/map.rs") | .from'

The public surface of the whole crate:

    percept maps show code --kind function --kind type \
      | jq -r 'select(.properties.public=="true") | .name'

## Rules

Filter before you print. `percept maps show code` with no filter is the
one thing that wastes context here.

Output is JSONL for `jq`. `--format md` prints a per-kind Markdown
outline instead - a human read, not a pipe.

The code map is read-only. `percept maps add-node code ...` is refused;
change the code, and the map follows.

Names are the join key. A symbol's name carries its file, so grep on
the name is a grep by file and by symbol at once.

Pipe stdout alone into `jq`. `percept` prints errors on stderr and
exits non-zero, so `2>&1 | jq` feeds jq an error line it cannot parse
and hides the real cause. When `--around kind:name` finds no such node,
the stderr line lists the near-matches as `kind:name` to retry with.
