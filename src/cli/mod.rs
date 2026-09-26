//! The command-line surface: `percept events search` queries the log,
//! `percept events show` dereferences one event by id, `percept maps`
//! folds a cognitive map from the log and prints it, `percept add` and
//! `percept remove` write a node or an edge with no map name - the
//! kind resolves it - `percept ask` runs one full turn -
//! including the tool loop - and prints the reply, `percept hook
//! <client>` records one coding client's turn from
//! the hook JSON it reads on stdin - see `hook` - and `percept init
//! <client>` writes that client's project config so its hooks call
//! `percept hook <client>` - see `init`. A
//! presentation-layer peer of `tui` - it forwards parsed input to
//! `store` and `app`, and has no chat logic of its own: `ask` drives the
//! same `AppService` turn policy `tui` does, just inline instead of over
//! a channel.
//!
//! `search` and `show` are the query primitive a model composes with:
//! every line is JSONL, for a caller piping into `jq`, never a table or
//! prose. `search`'s default line shortens long strings in the payload,
//! so a caller spends tokens on the whole of one deliberately, via
//! `--full`, `show`, or `show --range` into one `content`.

use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};

use crate::core::{
    cited_label, edge_kind_declared, schema_of_node_kind, schema_of_ref, Event, EventId, EventLog,
    EventQuery, EventSearch, Map, Mutation, Node, NodeId, NodeRef, Payload, Schema, Schemas,
};
use crate::mapstore;
use crate::shared::{parse_time, Timestamp};
use crate::store;
use crate::workspace;

/// A `--since`/`--until` value, refused by the flag a reader typed.
fn moment(flag: &str, s: &str) -> Result<Timestamp, String> {
    parse_time(s).ok_or_else(|| format!("invalid --{flag} value {s}"))
}

#[cfg(feature = "lab")]
mod turn;
#[cfg(feature = "lab")]
pub use turn::{run_turn, AskArgs};

#[derive(Parser)]
#[command(name = "percept")]
#[command(version = env!("PERCEPT_VERSION"))]
#[command(about = "Record what happens across your tools, so a model can query it.")]
#[command(long_about = "\
Record what happens across your tools, so a model can query it.

percept keeps an append-only log of events - prompts, replies, and tool \
calls - in $PERCEPT_HOME/percept.jsonl, ~/.percept by default, shared \
by every project; each event names the project it came from. It never \
ranks, summarises, or answers: its job is to make looking cheap and \
leave relevance to the caller.

A bare `percept` prints the start screen: how to record, then this \
project's maps whole - the same text a coding client reads when its \
session opens. `events search` and `events show` query the log, `maps \
list` and `maps show` print a cognitive map folded from it, `add` and \
`remove` write a node or an edge with no map name - the kind resolves \
it - and `maps change-node` changes one already there, `hook <client>` \
records one coding client's turn from the hook JSON it reads on stdin, \
and `init <client>` writes that client's project config to call it.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Work with the event log directly.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Read percept's maps - the cognitive ones folded from the log.
    Maps {
        #[command(subcommand)]
        command: MapsCommand,
    },
    /// Add a node or an edge - a document on stdin for several at once.
    /// The kind names the map; no write names one.
    Add(AddArgs),
    /// Remove a node or an edge - the inverse of `add`.
    Remove(RemoveArgs),
    /// Run one turn headlessly and print the reply.
    #[cfg(feature = "lab")]
    Ask(AskArgs),
    /// Open percept's own coding agent in the terminal.
    #[cfg(feature = "lab")]
    Code,
    /// Record one coding client's turn from the hook JSON it sends on
    /// stdin. Never fails the client's turn: an error prints to
    /// stderr and still exits with a JSON object on stdout.
    Hook(hook::HookArgs),
    /// Write a coding client's project config so its hooks call
    /// `percept hook <client>`.
    Init(init::InitArgs),
    /// Open percept in a browser: an HTTP server on `127.0.0.1` serving
    /// the embedded page, until the process is killed.
    Web,
}

#[derive(Subcommand)]
pub enum MapsCommand {
    /// Every map with its purpose, size, and kinds; `--json` for one
    /// object per line.
    List(ListMapsArgs),
    /// One map as Markdown; `--json` for its nodes, then its edges, one
    /// object per line.
    Show(ShowMapArgs),
    /// Change a node already in a map - a rename, a property, or both.
    /// Every change is subject to the rank rule a removal has: a node
    /// the user wrote, or last changed, takes no change from an agent.
    /// Prints the node's id.
    ChangeNode(ChangeNodeArgs),
}

#[derive(Args)]
pub struct ShowMapArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
    /// Print one JSON object per line - map, then node, then edge -
    /// instead of the default Markdown.
    #[arg(long)]
    json: bool,
    /// Repeatable. Keep only nodes of any of these kinds, and the edges
    /// between them.
    #[arg(long)]
    kind: Vec<String>,
    /// `kind:name` of a node, or the short id its map shows it as,
    /// `d41`. Keep only it and its neighbourhood, reached along edges
    /// in either direction.
    #[arg(long, value_parser = non_blank)]
    around: Option<String>,
    /// How many edges out `--around` reaches; 0 is the node alone.
    #[arg(long, default_value_t = 1, requires = "around")]
    depth: usize,
    /// Keep only what the map gained since this instant - an ISO-8601
    /// timestamp, or `<N>d`, `<N>h`, `<N>m` back from now: the nodes
    /// added since, and the ends of the edges added since.
    #[arg(long, value_parser = |s: &str| moment("since", s))]
    since: Option<Timestamp>,
    /// Fold every path's events instead of only this one's, printing
    /// one map per path. A node named with `--around` lives in one
    /// path's map, so the two do not combine.
    #[arg(long, conflicts_with = "around")]
    all_paths: bool,
}

#[derive(Args)]
pub struct ListMapsArgs {
    /// Fold every path's events instead of only this one's, printing
    /// one map per path.
    #[arg(long)]
    all_paths: bool,
    /// Print one JSON object per line instead of the default Markdown
    /// table.
    #[arg(long)]
    json: bool,
}

/// What every map change names: the map, and the events it was drawn
/// from.
#[derive(Args)]
pub struct MapArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
    /// Repeatable. An event this fact was drawn from.
    #[arg(long)]
    source: Vec<String>,
    /// Who is writing: `human` for a person at the terminal, `agent`
    /// for a model recording on their behalf. The map shows the
    /// difference.
    #[arg(long, default_value = "human", value_parser = parse_actor_word)]
    actor: String,
}

/// Checks `s` names an actor `store::parse_actor` knows, without
/// resolving it yet - the human's id isn't known until the log is
/// open, well after clap has parsed the command line.
fn parse_actor_word(s: &str) -> Result<String, String> {
    match s {
        "human" | "agent" | "system" | "user" | "model" => Ok(s.to_string()),
        other => Err(format!("{other:?} names no actor; use human, agent or system")),
    }
}

/// `percept add <kind> ...`'s whole tail, taken raw: a node kind then
/// its quoted name, an edge kind then the two nodes it joins, or
/// nothing, to read a document from stdin - see `add`. Captured raw
/// rather than declared on clap, since a property's name comes from
/// the schemas at runtime: `parse_write_args` splits it into the
/// positionals and the `--actor`/`--source`/`--causation`/property
/// flags among them.
#[derive(Args)]
#[command(after_help = "\
Examples:
  # A node: a node kind, its name, then a flag per property
  percept add concept \"Snapshot\" --definition \"the working tree saved under a prompt\"

  # An edge: an edge kind, then the two nodes it joins
  percept add covers c1 c3

  # Several nodes and edges at once, from a document on stdin
  percept add --actor agent --source <event> <<'EOF'
  concept \"Snapshot\"
    definition \"the working tree saved under a prompt\"
  EOF")]
pub struct AddArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `percept remove <kind> ...`'s whole tail, taken raw the same way
/// `AddArgs` is: a node kind then the node to remove, or an edge kind
/// then the two nodes it joins - see `remove`.
#[derive(Args)]
#[command(after_help = "\
Examples:
  # A node
  percept remove concept c3

  # An edge
  percept remove covers c1 c3")]
pub struct RemoveArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

/// `--actor`, `--source`, and `--causation` - the three flags every
/// write takes, parsed out of `AddArgs`'s or `RemoveArgs`'s raw tail by
/// `parse_write_args`.
struct WriteArgs {
    actor: String,
    source: Vec<String>,
    causation: Option<String>,
}

impl Default for WriteArgs {
    fn default() -> Self {
        Self {
            actor: "human".to_string(),
            source: Vec::new(),
            causation: None,
        }
    }
}

/// `parse_write_args`'s result: the positionals, in order, the fixed
/// flags, and every other `--<name>` flag as a property.
type ParsedWriteArgs = (Vec<String>, WriteArgs, BTreeMap<String, String>);

/// Splits `args` into its positionals, in order, and its flags:
/// `--actor`, `--source` (repeatable), and `--causation` are fixed,
/// read into a `WriteArgs`; any other `--<name>` is a property, read
/// into `properties`. Each flag takes its value as the next token, or
/// inline after `=`. A name given twice, fixed or not, is an error -
/// `--source` aside, the one flag repetition means something.
fn parse_write_args(args: Vec<String>) -> Result<ParsedWriteArgs, Box<dyn std::error::Error>> {
    let mut positionals = Vec::new();
    let mut write = WriteArgs::default();
    let mut actor_given = false;
    let mut causation_given = false;
    let mut properties = BTreeMap::new();

    let mut tokens = args.into_iter();
    while let Some(token) = tokens.next() {
        let Some(rest) = token.strip_prefix("--") else {
            positionals.push(token);
            continue;
        };
        let (name, inline) = match rest.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (rest.to_string(), None),
        };
        let value = match inline {
            Some(value) => value,
            None => tokens
                .next()
                .ok_or_else(|| format!("--{name} needs a value"))?,
        };
        match name.as_str() {
            "actor" => {
                if actor_given {
                    return Err("--actor given twice".into());
                }
                write.actor = parse_actor_word(&value)?;
                actor_given = true;
            }
            "source" => write.source.push(value),
            "causation" => {
                if causation_given {
                    return Err("--causation given twice".into());
                }
                write.causation = Some(value);
                causation_given = true;
            }
            other => {
                if properties.insert(other.to_string(), value).is_some() {
                    return Err(format!("--{other} given twice").into());
                }
            }
        }
    }
    Ok((positionals, write, properties))
}

/// The error every write gives properties it takes none of: an edge,
/// or a document naming no kind on the command line at all.
fn no_properties_expected(properties: &BTreeMap<String, String>) -> Box<dyn std::error::Error> {
    let names: Vec<String> = properties.keys().map(|name| format!("--{name}")).collect();
    format!("this write takes no properties; found {}", names.join(", ")).into()
}

/// `~` for a global schema, whose map lives at `$HOME` rather than the
/// project - `Schemas::global_root` says which - `project` otherwise.
/// What a written node's line names beside its map.
fn level_label(schemas: &dyn Schemas, map: &str) -> &'static str {
    if schemas.global_root(map).is_some() {
        "~"
    } else {
        "project"
    }
}

/// The line a written node prints: its short id, then the map it
/// landed in and the level its root names - `c3  concepts (project)`.
/// What `add`'s node form prints alone, and what `add`'s document form
/// prints ahead of each node line, so a reader always learns which map
/// a bare kind resolved to.
fn node_written_line(schemas: &dyn Schemas, map: &str, short_id: &str) -> String {
    format!("{short_id}  {map} ({})", level_label(schemas, map))
}

/// The error `add`/`remove` give a word no schema declares as a node
/// or an edge kind: every kind declared, csv, or - with no schema
/// loaded at all - the hint to run `percept init <client>`, the same
/// onboarding a bare `percept` gives.
fn unknown_kind(schemas: &dyn Schemas, kind: &str) -> Box<dyn std::error::Error> {
    if schemas.folded().is_empty() {
        return format!("no map declares {kind:?}; run percept init <client> to add one").into();
    }
    let kinds: Vec<&str> = schemas
        .folded()
        .iter()
        .flat_map(|schema| schema.node_kind_names().chain(schema.edge_kind_names()))
        .collect();
    format!("no map declares {kind:?}; kinds are {}", kinds.join(", ")).into()
}

/// The one schema `from` and `to` both resolve to - by `kind:name` or
/// by the short id each takes, via `schema_of_ref` - or the refusal an
/// edge across two maps gives, naming both. What `add`/`remove` find
/// an edge's map through, with no map name in the command: an edge
/// kind may repeat across schemas, so it says nothing on its own.
fn resolve_edge_map(
    schemas: &dyn Schemas,
    from: &str,
    to: &str,
) -> Result<Arc<Schema>, Box<dyn std::error::Error>> {
    let unknown = |ref_: &str| -> Box<dyn std::error::Error> {
        format!("{ref_:?} names no node kind or short id prefix any schema declares").into()
    };
    let from_schema = schema_of_ref(schemas, from).ok_or_else(|| unknown(from))?;
    let to_schema = schema_of_ref(schemas, to).ok_or_else(|| unknown(to))?;
    if from_schema.name() != to_schema.name() {
        return Err(format!(
            "{from:?} is in map {:?} but {to:?} is in map {:?}; an edge stays inside one map",
            from_schema.name(),
            to_schema.name()
        )
        .into());
    }
    Ok(from_schema)
}

#[derive(Args)]
pub struct ChangeNodeArgs {
    #[command(flatten)]
    target: MapArgs,
    /// `kind:name` of the node to change, or the short id its map
    /// shows it as, `d41`.
    #[arg(long, value_parser = non_blank)]
    node: String,
    /// A new name for the node.
    #[arg(long)]
    name: Option<String>,
    /// Repeatable `key=value`.
    #[arg(long = "prop", value_parser = parse_prop)]
    prop: Vec<(String, String)>,
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Search events, one JSON object per line, oldest first.
    Search(SearchArgs),
    /// Print one event by id.
    Show(ShowArgs),
}

#[derive(Args, Default)]
#[command(after_help = "\
Examples:
  # The 20 most recent events, printed oldest first
  percept events search --size 20

  # What the model did in the last day
  percept events search --since 1d --type tool.called

  # Prompts and replies naming a deploy, 300 characters around each hit
  percept events search --contains deploy --type message.received --preview 300

  # The same, with full payloads
  percept events search --contains deploy --type message.received --full

  # Two windows that tile with no gap or overlap
  percept events search --since 2d --until 1d
  percept events search --since 1d")]
pub struct SearchArgs {
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Inclusive.
    #[arg(long)]
    since: Option<String>,
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Exclusive.
    #[arg(long)]
    until: Option<String>,
    /// Repeatable. An event matching any of these sources passes.
    #[arg(long)]
    source: Vec<String>,
    /// Repeatable. An event matching any of these actors passes.
    #[arg(long, value_parser = parse_actor_word)]
    actor: Vec<String>,
    /// Repeatable. An event matching any of these types passes.
    #[arg(long = "type")]
    kind: Vec<String>,
    /// Repeatable. An event whose payload carries any of these
    /// substrings, case-insensitively, passes.
    #[arg(long, value_parser = non_blank)]
    contains: Vec<String>,
    /// Keep only the N most recent matching events. Output still runs
    /// oldest first.
    #[arg(long)]
    size: Option<usize>,
    /// How many characters of `content` a line keeps, cut around the
    /// first `--contains` hit when there is one.
    #[arg(long, default_value_t = store::PREVIEW_CHARS, value_parser = at_least_one, conflicts_with = "full")]
    preview: usize,
    /// Print the whole wire event per line instead of the constant-size
    /// default.
    #[arg(long)]
    full: bool,
}

#[derive(Args)]
pub struct ShowArgs {
    id: String,
    /// A character range `START:END` into `payload.content`, `END`
    /// exclusive; omit `START` to begin at 0 and `END` to reach the end
    /// of `content`, e.g. `400:`. Only event kinds that carry `content`
    /// support a range.
    #[arg(long, value_parser = parse_range)]
    range: Option<(Option<usize>, Option<usize>)>,
}

/// Parses `--range START:END`; either side may be blank.
fn parse_range(s: &str) -> Result<(Option<usize>, Option<usize>), String> {
    let (start, end) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid range {s:?}, expected START:END"))?;
    let bound = |name: &str, text: &str| {
        if text.is_empty() {
            Ok(None)
        } else {
            text.parse()
                .map(Some)
                .map_err(|_| format!("invalid range {name} {text:?}"))
        }
    };
    Ok((bound("start", start)?, bound("end", end)?))
}

/// A window of zero characters shows nothing and reads as a mistake.
fn at_least_one(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("must be at least 1".to_string()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
}

/// Rejects a blank value at parse time. A source that names nobody, a
/// search term contained by everything, a prompt that asks nothing -
/// each looks deliberate to a reader while meaning nothing.
pub(crate) fn non_blank(s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        return Err("must not be blank".to_string());
    }
    Ok(s.to_string())
}

/// Parses `--prop key=value`, split on the first `=` so a value may
/// carry one itself.
fn parse_prop(s: &str) -> Result<(String, String), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("invalid --prop {s:?}, expected key=value"))?;
    Ok((non_blank(key)?, value.to_string()))
}

/// `s` - `kind:name`, or the short id `map`'s own render shows it as -
/// resolved to the canonical `kind:name` a `Mutation` takes.
/// `Map::apply` resolves it again, against whatever the log holds by
/// the time the commit under its lock runs; the gap between this fold
/// and that one is the same one a plain `kind:name` reference always
/// lived with.
fn resolve_ref(map: &Map, s: &str) -> Result<NodeRef, Box<dyn std::error::Error>> {
    Ok(NodeRef::from(resolve_node(map, s)?))
}

/// The node `s` names - see `resolve_ref` - for a caller that needs
/// more of it than its ref.
fn resolve_node<'a>(map: &'a Map, s: &str) -> Result<&'a Node, Box<dyn std::error::Error>> {
    let id = map.resolve_str(s)?;
    Ok(map.node(id).expect("resolve_str returns a live node's id"))
}

/// What a document's block resolves to: a write to apply, or a node
/// already in the map that the block only hangs edges under.
enum Target {
    Written(Mutation),
    Standing(NodeId),
}

/// One fold of `name`, taken before a write's own atomic commit
/// re-folds it - just enough to resolve refs against, so a caller
/// that only needs the map never has to name the discarded creation
/// event `Snapshot::for_write` also returns.
fn map_for(
    schemas: &dyn Schemas,
    name: &str,
    source: &crate::core::Source,
    log: &dyn EventLog,
) -> Result<mapstore::Snapshot, Box<dyn std::error::Error>> {
    Ok(mapstore::Snapshot::for_write(schemas, name, source, log.load()?)?.1)
}

/// Resolves `path` inside `workspace`, refusing one outside it, and
/// reads `excerpt` from the tree when the caller gave none, refusing a
/// binary file, a range past the file's end, or an excerpt that is
/// blank after trimming - either given or read, since a blank excerpt
/// would match any text a later check compared it to. `path` is stored
/// resolved and repo-relative; an `excerpt` the caller did give is
/// otherwise stored as given.
fn build_file_cited(
    workspace: &workspace::Workspace,
    path: &str,
    lines: Option<(u32, u32)>,
    excerpt: Option<String>,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let resolved = workspace.resolve(path)?;
    let stored_path = PathBuf::from(workspace.relative(&resolved));

    let excerpt = match excerpt {
        Some(excerpt) => excerpt,
        None => {
            let text = workspace::read_text_lossy(&resolved)?;
            match lines {
                Some((from, to)) => {
                    let file_lines = text.lines().count();
                    if to as usize > file_lines {
                        return Err(format!(
                            "lines {from}-{to} run past {}'s {file_lines} lines",
                            stored_path.display()
                        )
                        .into());
                    }
                    text.lines()
                        .skip(from as usize - 1)
                        .take((to - from + 1) as usize)
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                None => text,
            }
        }
    };
    if excerpt.trim().is_empty() {
        return Err(format!("{}'s excerpt must not be blank", stored_path.display()).into());
    }

    Ok(Payload::FileCited {
        path: stored_path,
        lines,
        excerpt,
    })
}

fn known_event_id(
    id: &str,
    log: &dyn EventLog,
) -> Result<crate::core::EventId, Box<dyn std::error::Error>> {
    Ok(store::find_event(log, id)?.id())
}

/// Searches `log` for events matching `args`, printing one JSON object
/// per line in log order. `store` owns the wire shape; the CLI only
/// builds the query and formats the result.
pub fn search(
    args: SearchArgs,
    log: &dyn EventSearch,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query(&args, me)?;
    let events = log.search(&query)?;

    print_lines(events.iter().map(|event| {
        if args.full {
            store::encode(event)
        } else {
            store::summarize(event, query.hit(event), args.preview)
        }
    }))
}

/// Prints one JSONL line per item. One buffered writer rather than a
/// syscall per line: the caller is a pipe into jq, and a whole-log
/// listing runs to thousands of lines.
fn print_lines(lines: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    for line in lines {
        if let Err(e) = writeln!(out, "{line}") {
            return stop_if_pipe_closed(e);
        }
    }
    out.flush().or_else(stop_if_pipe_closed)
}

/// Writes one block of text to stdout verbatim - a rendered Markdown
/// map already carries its own newlines.
fn print_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    if let Err(e) = write!(out, "{text}") {
        return stop_if_pipe_closed(e);
    }
    out.flush().or_else(stop_if_pipe_closed)
}

/// Runs `print` for each path `maps list` and `maps show` fold over:
/// only `root` by default; with `--all-paths`, every distinct path in
/// `events`, each under a marker naming it - a `path <path>` line in
/// Markdown, a `{"path": ...}` line in JSON - since the maps of two
/// paths look alike, short ids included.
fn per_path(
    all_paths: bool,
    json: bool,
    root: &Path,
    events: &[crate::core::Event],
    mut print: impl FnMut(&Path) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if !all_paths {
        return print(root);
    }
    for (i, path) in mapstore::paths(events).iter().enumerate() {
        let marker = if json {
            format!("{}\n", serde_json::json!({ "path": path }))
        } else if i == 0 {
            format!("path {}\n\n", path.display())
        } else {
            format!("\npath {}\n\n", path.display())
        };
        print_text(&marker)?;
        print(path)?;
    }
    Ok(())
}

/// A bare `percept`: prints `start_text`.
pub fn start(log: &dyn EventLog, schemas: &dyn Schemas, project: &Path) -> Result<(), Box<dyn std::error::Error>> {
    print_text(&start_text(log, schemas, project)?)
}

/// The start screen - `mapstore::start` over every map of `project`,
/// folded from one read of `log`. What a bare `percept` prints and
/// what `hook` answers a coding client's `SessionStart` with.
pub fn start_text(log: &dyn EventLog, schemas: &dyn Schemas, project: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let maps = mapstore::fold_all_at(schemas, &events, project)?;
    Ok(mapstore::start(schemas, &maps))
}

/// Prints every map percept knows with its size: the log's maps, folded
/// from one read of `log` at `project`'s path, or at every path with
/// `--all-paths`.
pub fn maps_list(
    args: ListMapsArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    project: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = log.load()?;
    per_path(args.all_paths, args.json, project, &events, |path| {
        let maps = mapstore::fold_all_at(schemas, &events, path)?;
        if args.json {
            print_lines(maps.iter().map(mapstore::encode_map))
        } else {
            print_text(&mapstore::catalogue(&maps))
        }
    })
}

/// Prints the map `args.map` names, nodes then edges. `--around` cuts
/// it to a neighbourhood first, then `--kind` cuts that to its kinds,
/// so a node of another kind still counts as a step on the way.
pub fn maps_show(
    args: ShowMapArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = log.load()?;
    per_path(args.all_paths, args.json, root, &events, |path| {
        print_map(mapstore::fold_map_at(schemas, &args.map, &events, path)?, &args)
    })
}

/// `maps_show`'s tail: cut `map` to `args`'s filters, then print it
/// nodes-then-edges. `--since` runs after `--around`, so it reads as
/// "what changed near this node".
fn print_map(map: Map, args: &ShowMapArgs) -> Result<(), Box<dyn std::error::Error>> {
    // An empty map has nothing to resolve `--around` against - `select`'s
    // own empty-map case skips it anyway, so a node named on one is not
    // an error to report over "nothing recorded yet".
    let around = if map.nodes().is_empty() {
        None
    } else {
        args.around
            .as_deref()
            .map(|s| resolve_ref(&map, s))
            .transpose()?
    };
    let selection = crate::core::Selection {
        around: around.as_ref().map(|node| (node, args.depth)),
        since: args.since,
        kinds: &args.kind,
        ..crate::core::Selection::default()
    };
    let fragment = map.select(&selection)?;
    if !selection.is_whole() {
        eprintln!("{}", mapstore::encode_fragment(&fragment));
    }
    if args.json {
        print_lines(mapstore::encode_lines(fragment.map(), true))
    } else {
        print_text(&mapstore::markdown(fragment.map()))
    }
}

/// One map change from the shell: `target`'s cited events resolved and
/// `mutation` checked, applied, and committed as `target.actor`
/// (`human` by default), caused by `cause`, all under `mapstore::commit`'s
/// one lock. `me` resolves `human`/`user` to this log's own `HumanId`.
/// Returns the payload, for `add-node` to print the minted id.
fn write(
    target: MapArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let MapArgs {
        map,
        source: cited,
        actor,
    } = target;
    let actor = store::parse_actor(&actor, me)?;
    let event = mapstore::commit(log, schemas, &map, source, &cited, actor, cause, mutation)?;
    Ok(event.payload().clone())
}

/// One write under `map`'s name, with no `MapArgs` to carry it:
/// `write.actor` parsed against `me`, `write.source` resolved and
/// threaded to `mutation`, caused by `write.causation` when given, else
/// `default_cause` - `write`'s shape for `add` and `remove`, which
/// resolve the map from the kind rather than reading it off a flag.
#[allow(clippy::too_many_arguments)]
fn commit_write(
    map: &str,
    write: &WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    default_cause: Option<EventId>,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let actor = store::parse_actor(&write.actor, me)?;
    let causation = resolve_causation(write.causation.as_deref(), log, default_cause)?;
    let event = mapstore::commit(log, schemas, map, source, &write.source, actor, causation, mutation)?;
    Ok(event.payload().clone())
}

/// `write.causation`, resolved to a known event, when given - else
/// `default`, the coding client turn's own cause. The same rule
/// `maps record` always gave an explicit `--causation`.
fn resolve_causation(
    explicit: Option<&str>,
    log: &dyn EventLog,
    default: Option<EventId>,
) -> Result<Option<EventId>, Box<dyn std::error::Error>> {
    Ok(explicit.map(|id| known_event_id(id, log)).transpose()?.or(default))
}

/// `percept add <kind> ...` - a node, when `kind` names one; an edge,
/// when it names one instead; a document on stdin, with no kind at
/// all. See `AddArgs` for the raw tail `parse_write_args` splits, and
/// `docs/cli.md` for the shape a session types.
pub fn add(
    args: AddArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    checkout: &Path,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (positionals, write, properties) = parse_write_args(args.args)?;
    let Some(kind) = positionals.first().cloned() else {
        if !properties.is_empty() {
            return Err(no_properties_expected(&properties));
        }
        let mut document = String::new();
        io::stdin().read_to_string(&mut document)?;
        return record_document(&document, write, log, schemas, source, checkout, me, cause);
    };

    if let Some(schema) = schema_of_node_kind(schemas, &kind) {
        add_node(schema, kind, positionals, properties, write, log, schemas, source, me, cause)
    } else if edge_kind_declared(schemas, &kind) {
        add_edge(kind, positionals, properties, write, log, schemas, source, me, cause)
    } else {
        Err(unknown_kind(schemas, &kind))
    }
}

/// `add`'s node form: `kind "<name>" --<property> "<value>" ...`.
/// Prints the minted node's short id, map, and level - see
/// `node_written_line`.
#[allow(clippy::too_many_arguments)]
fn add_node(
    schema: Arc<Schema>,
    kind: String,
    positionals: Vec<String>,
    properties: BTreeMap<String, String>,
    write: WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    if positionals.len() != 2 {
        return Err(format!("expected `{kind} \"<name>\"`").into());
    }
    let name = positionals[1].clone();
    let map = schema.name().to_string();
    let payload = commit_write(&map, &write, log, schemas, source, me, cause, |sources| {
        Mutation::AddNode { kind, name, properties, sources }
    })?;
    if let Payload::NodeAdded { node, .. } = &payload {
        let folded = mapstore::fold_map(log, schemas, &map, &source.path)?;
        let short_id = folded.short_id(*node).unwrap_or_default();
        println!("{}", node_written_line(schemas, &map, &short_id));
    }
    Ok(())
}

/// `add`'s edge form: `kind <from> <to>`, resolved against one fold of
/// the map `resolve_edge_map` finds from `from` and `to`, taken before
/// the write's own atomic commit re-folds it. An edge takes no
/// properties.
#[allow(clippy::too_many_arguments)]
fn add_edge(
    kind: String,
    positionals: Vec<String>,
    properties: BTreeMap<String, String>,
    write: WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    if positionals.len() != 3 {
        return Err(format!("expected `{kind} <from> <to>`").into());
    }
    if !properties.is_empty() {
        return Err(no_properties_expected(&properties));
    }
    let (from, to) = (positionals[1].clone(), positionals[2].clone());
    let schema = resolve_edge_map(schemas, &from, &to)?;
    let map = schema.name().to_string();
    let snapshot = map_for(schemas, &map, source, log)?;
    let from = resolve_ref(snapshot.map(), &from)?;
    let to = resolve_ref(snapshot.map(), &to)?;
    commit_write(&map, &write, log, schemas, source, me, cause, |sources| {
        Mutation::AddEdge { kind, from, to, sources }
    })
    .map(drop)
}

/// `percept remove <kind> ...` - the inverse of `add`: a node, when
/// `kind` names one; an edge, when it names one instead. No document
/// form.
pub fn remove(
    args: RemoveArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (positionals, write, properties) = parse_write_args(args.args)?;
    if !properties.is_empty() {
        return Err(no_properties_expected(&properties));
    }
    let Some(kind) = positionals.first().cloned() else {
        return Err("expected a node kind and a node, or an edge kind and two nodes".into());
    };

    if let Some(schema) = schema_of_node_kind(schemas, &kind) {
        remove_node(schema, kind, positionals, write, log, schemas, source, me, cause)
    } else if edge_kind_declared(schemas, &kind) {
        remove_edge(kind, positionals, write, log, schemas, source, me, cause)
    } else {
        Err(unknown_kind(schemas, &kind))
    }
}

/// `remove`'s node form: `kind <node>`, dropping the edges that touch
/// it too.
#[allow(clippy::too_many_arguments)]
fn remove_node(
    schema: Arc<Schema>,
    kind: String,
    positionals: Vec<String>,
    write: WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    if positionals.len() != 2 {
        return Err(format!("expected `{kind} <node>`").into());
    }
    let map = schema.name().to_string();
    let snapshot = map_for(schemas, &map, source, log)?;
    let node = resolve_ref(snapshot.map(), &positionals[1])?;
    commit_write(&map, &write, log, schemas, source, me, cause, |sources| {
        Mutation::RemoveNode { node, sources }
    })
    .map(drop)
}

/// `remove`'s edge form: `kind <from> <to>`.
#[allow(clippy::too_many_arguments)]
fn remove_edge(
    kind: String,
    positionals: Vec<String>,
    write: WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    if positionals.len() != 3 {
        return Err(format!("expected `{kind} <from> <to>`").into());
    }
    let (from, to) = (positionals[1].clone(), positionals[2].clone());
    let schema = resolve_edge_map(schemas, &from, &to)?;
    let map = schema.name().to_string();
    let snapshot = map_for(schemas, &map, source, log)?;
    let from = resolve_ref(snapshot.map(), &from)?;
    let to = resolve_ref(snapshot.map(), &to)?;
    commit_write(&map, &write, log, schemas, source, me, cause, |sources| {
        Mutation::RemoveEdge { kind, from, to, sources }
    })
    .map(drop)
}

/// Changes a node already in a map - a rename, a property, or both -
/// and prints the event id, the way `maps add-node` prints the node it
/// minted.
pub fn maps_change_node(
    args: ChangeNodeArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ChangeNodeArgs {
        target,
        node,
        name,
        prop,
    } = args;
    let snapshot = map_for(schemas, &target.map, source, log)?;
    let node = resolve_ref(snapshot.map(), &node)?;
    let payload = write(target, log, schemas, source, me, cause, |sources| {
        Mutation::ChangeNode {
            node,
            name,
            properties: prop.into_iter().collect::<BTreeMap<_, _>>(),
            sources,
        }
    })?;
    if let Payload::NodeChanged { node, .. } = &payload {
        println!("{}", node.as_uuid());
    }
    Ok(())
}

/// One `cites` line under a node: the file it rested on, and the range
/// within it, `None` for the whole file.
struct DocCite {
    path: String,
    lines: Option<(u32, u32)>,
}

/// One edge line under a node: its kind, and the ref its target names -
/// a short id or a bare kind name, resolved once against the map
/// `maps_record` loaded before any write.
struct DocEdge {
    kind: String,
    target: String,
    line: usize,
}

/// One node a document names, with what is declared under it - a fresh
/// node (`kind` its node kind, `name` its name) or, when `is_change` is
/// set, a change to one already in the map (`kind` its short id,
/// `name` unused).
struct DocNode {
    kind: String,
    name: String,
    is_change: bool,
    properties: BTreeMap<String, String>,
    edges: Vec<DocEdge>,
    cites: Vec<DocCite>,
    line: usize,
}

/// Splits `s` at its first run of whitespace, trimming what leads the
/// rest - `word`, then whatever follows it on the line.
fn split_first_word(s: &str) -> Option<(&str, &str)> {
    let (word, rest) = s.trim_start().split_once(char::is_whitespace)?;
    Some((word, rest.trim_start()))
}

/// Parses a double-quoted value starting at `s`'s first character:
/// `\"` inside is a literal quote, and nothing may follow the closing
/// one but whitespace. `None` for anything else, so a caller can turn
/// it into the line-numbered error a document's reader needs.
fn parse_quoted(s: &str) -> Option<String> {
    if !s.starts_with('"') {
        return None;
    }
    let mut value = String::new();
    let mut chars = s[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.clone().next() == Some('"') => {
                chars.next();
                value.push('"');
            }
            '"' => {
                let rest: String = chars.collect();
                return rest.trim().is_empty().then_some(value);
            }
            other => value.push(other),
        }
    }
    None
}

/// The reverse of `parse_quoted` - `name`, quoted and with every `"`
/// escaped, the way a node or edge line in `maps record`'s document
/// grammar writes it.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// A cite's path and, when present, the line range after its trailing
/// `:from-to`.
type CiteRange = (String, Option<(u32, u32)>);

/// `s`, split at a trailing `:from-to`, when the part after the last
/// `:` has that shape - a `cites` line's path may itself hold a `:`
/// that isn't a range. A suffix that looks like two numbers but breaks
/// the range's own rule (`store::parse_lines`'s) is an error rather
/// than being read back as part of the path.
fn split_cite_range(s: &str) -> Result<CiteRange, Box<dyn std::error::Error>> {
    let Some((path, range)) = s.rsplit_once(':') else {
        return Ok((s.to_string(), None));
    };
    let Some((from, to)) = range.split_once('-') else {
        return Ok((s.to_string(), None));
    };
    if from.parse::<u32>().is_err() || to.parse::<u32>().is_err() {
        return Ok((s.to_string(), None));
    }
    Ok((path.to_string(), Some(store::parse_lines(range)?)))
}

/// Parses `maps record`'s document grammar: a node line at column 0,
/// either `<kind> "<name>"`, which adds a node, or a bare short id,
/// `t4`, which starts a change to the node it names - each owns every
/// indented line under it - a `<key> "<value>"` property (`name
/// "<value>"` is a rename, only meaningful under a change), an `<edge
/// kind> <ref>`, or a `cites <path>[:<from>-<to>]` - until the next
/// node line or the document's end. A blank line is ignored; anything
/// else names its line number.
fn parse_document(text: &str) -> Result<Vec<DocNode>, Box<dyn std::error::Error>> {
    let mut nodes: Vec<DocNode> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() {
            continue;
        }
        if !raw.starts_with(char::is_whitespace) {
            let (word, rest) = match split_first_word(raw) {
                Some((word, rest)) => (word.to_string(), rest),
                None => (raw.trim().to_string(), ""),
            };
            let (kind, name, is_change) = if rest.is_empty() {
                (word, String::new(), true)
            } else {
                let name = parse_quoted(rest).ok_or_else(|| {
                    format!("line {line}: expected `<kind> \"<name>\"`, or a short id alone")
                })?;
                (word, name, false)
            };
            nodes.push(DocNode {
                kind,
                name,
                is_change,
                properties: BTreeMap::new(),
                edges: Vec::new(),
                cites: Vec::new(),
                line,
            });
            continue;
        }
        let node = nodes
            .last_mut()
            .ok_or_else(|| format!("line {line}: indented line has no node above it"))?;
        let (word, rest) = split_first_word(raw.trim())
            .ok_or_else(|| format!("line {line}: expected a property, an edge, or `cites`"))?;
        if word == "cites" {
            let (path, lines) =
                split_cite_range(rest).map_err(|err| format!("line {line}: {err}"))?;
            node.cites.push(DocCite { path, lines });
        } else if rest.starts_with('"') {
            let value = parse_quoted(rest)
                .ok_or_else(|| format!("line {line}: expected `{word} \"<value>\"`"))?;
            node.properties.insert(word.to_string(), value);
        } else {
            node.edges.push(DocEdge {
                kind: word.to_string(),
                target: rest.to_string(),
                line,
            });
        }
    }
    Ok(nodes)
}

/// The schema a document's node line resolves to: `node.kind` by its
/// node kind, for a fresh node; the short id `node.kind` holds, for a
/// change block - `schema_of_node_kind` and `schema_of_ref`
/// respectively, since a change block's `kind` field is a short id, not
/// a node kind name.
fn node_schema(schemas: &dyn Schemas, node: &DocNode) -> Result<Arc<Schema>, Box<dyn std::error::Error>> {
    let resolved = if node.is_change {
        schema_of_ref(schemas, &node.kind)
    } else {
        schema_of_node_kind(schemas, &node.kind)
    };
    resolved.ok_or_else(|| unknown_kind(schemas, &node.kind)).map_err(|err| format!("line {}: {err}", node.line).into())
}

/// `add`'s document form: every node and edge a document on stdin
/// declares, written to the one map its first line resolves to - see
/// `node_schema`. A later line resolving to another map is refused,
/// naming both, before anything is written. Every node and edge is then
/// checked and applied to one in-memory fold of that map -
/// `Map::apply` enforces known kinds, required properties, duplicate
/// names, and known edge kinds, so this only resolves an edge's ref,
/// live, against the map as it stands at that line: a bare kind name to
/// the last node of that kind this document declared above it, anything
/// else through `Map::resolve_str`. Nothing is appended until every
/// node and edge has passed, in one batch under the log's lock, so a
/// failure midway, whether a duplicate name, a missing `--source`, or a
/// bad ref, leaves nothing written; the error names the node or line it
/// reached.
#[allow(clippy::too_many_arguments)]
fn record_document(
    document: &str,
    write: WriteArgs,
    log: &dyn EventLog,
    schemas: &dyn Schemas,
    source: &crate::core::Source,
    checkout: &Path,
    me: Option<crate::core::HumanId>,
    cause: Option<EventId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = parse_document(document)?;
    let Some(first) = nodes.first() else {
        return Ok(());
    };
    let total = nodes.len();
    let map_schema = node_schema(schemas, first)?;
    for node in &nodes[1..] {
        let schema = node_schema(schemas, node)?;
        if schema.name() != map_schema.name() {
            return Err(format!(
                "line {}: this document already writes into {:?}; {:?} is in map {:?}",
                node.line,
                map_schema.name(),
                node.kind,
                schema.name()
            )
            .into());
        }
    }
    let map = map_schema.name().to_string();

    let node_sources: Vec<EventId> = write
        .source
        .iter()
        .map(|id| known_event_id(id, log))
        .collect::<Result<_, _>>()?;
    let causation_id = resolve_causation(write.causation.as_deref(), log, cause)?;
    // Only opened when the document has a `cites` line to resolve - a
    // document with none should still record against a `checkout` that
    // does not exist, the way it always could.
    let workspace = if nodes.iter().any(|node| !node.cites.is_empty()) {
        Some(workspace::Workspace::new(checkout)?)
    } else {
        None
    };

    let actor = store::parse_actor(&write.actor, me)?;
    let batch_source = source.clone();

    let events = mapstore::commit_batch(log, schemas, &map, source, move |snapshot| {
        let mut batch: Vec<Event> = Vec::new();
        let mut last_of_kind: HashMap<String, NodeId> = HashMap::new();

        for (i, node) in nodes.into_iter().enumerate() {
            let label = if node.is_change {
                format!("line {}: change {} of {total} ({})", node.line, i + 1, node.kind)
            } else {
                format!(
                    "line {}: node {} of {total} ({} {:?})",
                    node.line,
                    i + 1,
                    node.kind,
                    node.name
                )
            };
            let context = |err: Box<dyn std::error::Error>| -> Box<dyn std::error::Error> {
                format!("{label}: {err}").into()
            };

            let mut sources = node_sources.clone();
            for cite in &node.cites {
                let workspace = workspace
                    .as_ref()
                    .expect("a cite here means the document had one, so it was opened above");
                let payload =
                    build_file_cited(workspace, &cite.path, cite.lines, None).map_err(context)?;
                let event = Event::new(actor, batch_source.clone(), causation_id, payload);
                sources.push(event.id());
                batch.push(event);
            }

            // Either arm yields the kind and name the node has once it
            // lands, so one tail applies both.
            let cited = !node.cites.is_empty();
            let (target, kind, name) = if node.is_change {
                let standing = resolve_node(snapshot.map(), &node.kind).map_err(context)?;
                let (kind, old_name, standing) =
                    (standing.kind.clone(), standing.name.clone(), standing.id);
                let mut properties = node.properties;
                let rename = properties.remove("name");
                let name = rename.clone().unwrap_or_else(|| old_name.clone());
                // A block naming a short id and nothing but edges leaves
                // the node alone: the edges under it are their own
                // writes, and the document's own `--source` is about
                // what is being recorded, not about this node. A `cites`
                // line is about this node, so it is a change.
                let target = if rename.is_none() && properties.is_empty() && !cited {
                    Target::Standing(standing)
                } else {
                    Target::Written(Mutation::ChangeNode {
                        node: NodeRef {
                            kind: kind.clone(),
                            name: old_name,
                        },
                        name: rename,
                        properties,
                        sources,
                    })
                };
                (target, kind, name)
            } else {
                let mutation = Mutation::AddNode {
                    kind: node.kind.clone(),
                    name: node.name.clone(),
                    properties: node.properties,
                    sources,
                };
                (Target::Written(mutation), node.kind, node.name)
            };
            let node_id = match target {
                Target::Written(mutation) => {
                    let payload =
                        snapshot.apply(mutation, actor).map_err(|err| context(err.into()))?;
                    let node_id = match &payload {
                        Payload::NodeAdded { node, .. } | Payload::NodeChanged { node, .. } => *node,
                        _ => unreachable!("AddNode and ChangeNode yield a node payload"),
                    };
                    batch.push(Event::new(actor, batch_source.clone(), causation_id, payload));
                    node_id
                }
                Target::Standing(node_id) => node_id,
            };
            last_of_kind.insert(kind.clone(), node_id);
            let from_ref = NodeRef { kind, name };

            for edge in node.edges {
                let to_id = if snapshot.map().schema().node_kind(&edge.target).is_some() {
                    last_of_kind.get(&edge.target).copied().ok_or_else(|| {
                        format!(
                            "line {}: no {} declared earlier in this document",
                            edge.line, edge.target
                        )
                    })?
                } else {
                    snapshot
                        .map()
                        .resolve_str(&edge.target)
                        .map_err(|err| format!("line {}: {err}", edge.line))?
                };
                let to_node = snapshot
                    .map()
                    .node(to_id)
                    .expect("resolve_str and last_of_kind name a live node");
                let to_ref = NodeRef {
                    kind: to_node.kind.clone(),
                    name: to_node.name.clone(),
                };
                let mutation = Mutation::AddEdge {
                    kind: edge.kind,
                    from: from_ref.clone(),
                    to: to_ref,
                    sources: node_sources.clone(),
                };
                let payload = snapshot.apply(mutation, actor).map_err(|err| context(err.into()))?;
                batch.push(Event::new(actor, batch_source.clone(), causation_id, payload));
            }
        }

        Ok(batch)
    })?;

    let map_name = map;
    let map = mapstore::fold_map(log, schemas, &map_name, &source.path)?;
    let mut node_lines = Vec::new();
    let mut edge_lines = Vec::new();
    let mut cited_lines = Vec::new();
    for event in &events {
        match event.payload() {
            Payload::FileCited { path, lines, .. } => cited_lines.push(format!(
                "cited {} {}",
                event.id().as_uuid(),
                cited_label(path, *lines)
            )),
            Payload::NodeAdded { node, kind, name, .. } => {
                let short_id = map.short_id(*node).unwrap_or_default();
                let line = node_written_line(schemas, &map_name, &short_id);
                node_lines.push(format!("{line} {kind} {}", quote(name)));
            }
            Payload::NodeChanged { node, name, .. } => {
                let short_id = map.short_id(*node).unwrap_or_default();
                let line = node_written_line(schemas, &map_name, &short_id);
                match name {
                    Some(name) => node_lines.push(format!("{line} changed to {}", quote(name))),
                    None => node_lines.push(format!("{line} changed")),
                }
            }
            Payload::EdgeAdded { kind, from, to, .. } => {
                let from_short = map.short_id(*from).unwrap_or_default();
                let to_short = map.short_id(*to).unwrap_or_default();
                edge_lines.push(format!("{kind} {from_short} -> {to_short}"));
            }
            _ => {}
        }
    }

    print_lines(node_lines.into_iter().chain(edge_lines).chain(cited_lines))
}

/// A reader that stops early - `head`, or a `jq` that has seen enough -
/// closes the pipe. That is the caller's choice, not a failure to
/// report.
pub(super) fn stop_if_pipe_closed(e: io::Error) -> Result<(), Box<dyn std::error::Error>> {
    if e.kind() == io::ErrorKind::BrokenPipe {
        Ok(())
    } else {
        Err(e.into())
    }
}

/// The `EventQuery` `args` asks for, with every flag parsed once into
/// the value it is compared against. A filter naming something the log
/// has no word for is an error here rather than a query that quietly
/// matches nothing.
fn parse_query(args: &SearchArgs, me: Option<crate::core::HumanId>) -> Result<EventQuery, String> {
    let kinds = args
        .kind
        .iter()
        .map(|kind| store::parse_kind(kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let actors = args
        .actor
        .iter()
        .map(|actor| store::parse_actor(actor, me).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let since = args
        .since
        .as_deref()
        .map(|s| moment("since", s))
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(|s| moment("until", s))
        .transpose()?;

    let query = EventQuery {
        since,
        until,
        actors,
        sources: args.source.clone(),
        kinds,
        text: args.contains.clone(),
        size: args.size,
        ..Default::default()
    };
    if let Some((since, until)) = query.inverted_window() {
        return Err(format!("--since {since} is not before --until {until}"));
    }
    Ok(query)
}

/// Prints the one event `args.id` names. An id the log doesn't carry
/// fails loudly rather than printing nothing, so an empty result never
/// means "your id was wrong". With `--range`, prints `payload.content`
/// sliced to it instead of the whole event.
pub fn show(args: ShowArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let (start, end) = args.range.unwrap_or_default();
    println!("{}", store::read_event(log, &args.id, start, end)?);
    Ok(())
}

pub mod hook;
pub mod init;

#[cfg(test)]
mod tests;
