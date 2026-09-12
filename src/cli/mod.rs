//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events search` queries the log,
//! `percept events show` dereferences one event by id, `percept maps`
//! folds a cognitive map from the log and prints it, `percept ask`
//! runs one full turn - including the tool loop - and prints the
//! reply, `percept reflect` runs one asking the model to revise its
//! maps, `percept hook <client>` records one coding client's turn from
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

use clap::{Args, Parser, Subcommand};

use crate::core::{
    cited_label, Event, EventId, EventLog, EventQuery, EventSearch, Map, Mutation, Node, NodeId,
    NodeRef, Payload, Schemas,
};
use crate::mapstore;
use crate::shared::Timestamp;
use crate::store;
use crate::workspace;

#[cfg(feature = "lab")]
mod turn;
#[cfg(feature = "lab")]
pub use turn::{run_turn, AskArgs};

#[derive(Parser)]
#[command(name = "percept")]
#[command(about = "Record what happens across your tools, so a model can query it.")]
#[command(long_about = "\
Record what happens across your tools, so a model can query it.

percept keeps an append-only log of events - prompts, replies, and tool \
calls - in $PERCEPT_HOME/percept.jsonl, ~/.percept by default, shared \
by every project; each event names the project it came from. It never \
ranks, summarises, or answers: its job is to make looking cheap and \
leave relevance to the caller.

`events publish` appends one event, `events search` and `events show` \
query it, `maps list` and `maps show` print a cognitive map folded \
from it, `maps record` and `maps change-node` change one, `hook \
<client>` records one coding client's turn from the hook JSON it reads \
on stdin, and `init <client>` writes that client's project config to \
call it.")]
#[cfg_attr(not(feature = "lab"), command(arg_required_else_help = true))]
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
    /// Run one turn headlessly and print the reply.
    #[cfg(feature = "lab")]
    Ask(AskArgs),
    /// Run one turn asking the model to revise its maps from the log.
    #[cfg(feature = "lab")]
    Reflect,
    /// Record one coding client's turn from the hook JSON it sends on
    /// stdin. Never fails the client's turn: an error prints to
    /// stderr and still exits with a JSON object on stdout.
    Hook(hook::HookArgs),
    /// Write a coding client's project config so its hooks call
    /// `percept hook <client>`.
    Init(init::InitArgs),
    /// Open the review page: an HTTP server on `127.0.0.1` serving the
    /// embedded page, until the process is killed.
    Review,
    /// What this project has recorded, what needs attention, and where
    /// to go next.
    Start,
}

#[derive(Subcommand)]
pub enum MapsCommand {
    /// Every map with its purpose, size, and kinds; `--json` for one
    /// object per line.
    List(ListMapsArgs),
    /// One map as Markdown; `--json` for its nodes, then its edges, one
    /// object per line.
    Show(ShowMapArgs),
    /// Add a node to a map. Prints the minted node id.
    AddNode(AddNodeArgs),
    /// Add an edge between two nodes already in a map.
    AddEdge(EdgeArgs),
    /// Remove a node from a map, dropping the edges that touch it.
    RemoveNode(RemoveNodeArgs),
    /// Remove an edge from a map.
    RemoveEdge(RemoveEdgeArgs),
    /// Add several nodes and edges from a document on stdin, or change
    /// one already in the map - a margin line naming a short id, `t4`,
    /// starts a change block: `state "done"` under it sets a property,
    /// `name "..."` renames it, and an indented `why "..."` line under
    /// it sets the change's own why rather than a property. Prints one
    /// line per node or change, then one per edge, then one per `cites`
    /// line.
    Record(RecordArgs),
    /// Change a node already in a map - a rename, a property, or a
    /// `why`-only comment - subject to the same rank rule a rename or
    /// removal always has. Prints the node's id.
    ChangeNode(ChangeNodeArgs),
    /// One map's kinds, relations, and how to record to it, from its
    /// schema.
    Describe(DescribeMapArgs),
}

#[derive(Args)]
pub struct DescribeMapArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
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
    #[arg(long, value_parser = |s: &str| parse_time("since", s))]
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

#[derive(Args)]
pub struct AddNodeArgs {
    #[command(flatten)]
    target: MapArgs,
    #[arg(long)]
    kind: String,
    #[arg(long)]
    name: String,
    /// Repeatable `key=value`.
    #[arg(long = "prop", value_parser = parse_prop)]
    prop: Vec<(String, String)>,
}

#[derive(Args)]
pub struct RemoveNodeArgs {
    #[command(flatten)]
    target: MapArgs,
    /// `kind:name` of the node to remove, or the short id its map
    /// shows it as, `d41`.
    #[arg(long, value_parser = non_blank)]
    node: String,
    #[arg(long, value_parser = non_blank)]
    why: String,
}

/// An edge to add or remove - the same three things name it either way.
#[derive(Args)]
pub struct EdgeArgs {
    #[command(flatten)]
    target: MapArgs,
    #[arg(long)]
    kind: String,
    /// `kind:name` of the node the edge starts at, or the short id its
    /// map shows it as, `d41`.
    #[arg(long, value_parser = non_blank)]
    from: String,
    /// `kind:name` of the node the edge points to, or its short id.
    #[arg(long, value_parser = non_blank)]
    to: String,
}

/// `maps remove-edge`'s arguments: `EdgeArgs` plus why, required only
/// on removal - `maps add-edge` needs none.
#[derive(Args)]
pub struct RemoveEdgeArgs {
    #[command(flatten)]
    edge: EdgeArgs,
    #[arg(long, value_parser = non_blank)]
    why: String,
}

#[derive(Args)]
pub struct RecordArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
    /// Repeatable. An event this fact was drawn from. Added to every
    /// node's and every edge's sources, alongside a node's own `cites`.
    #[arg(long)]
    source: Vec<String>,
    /// Who is writing: `human` for a person at the terminal, `agent`
    /// for a model recording on their behalf.
    #[arg(long, default_value = "human", value_parser = parse_actor_word)]
    actor: String,
    /// The id of the event a `cites` line's `file.cited` event follows
    /// from.
    #[arg(long)]
    causation: Option<String>,
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
    /// Why this change was made.
    #[arg(long)]
    why: Option<String>,
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Append one event to the log.
    Publish(PublishArgs),
    /// Search events, one JSON object per line, oldest first.
    Search(SearchArgs),
    /// Print one event by id.
    Show(ShowArgs),
}

#[derive(Args)]
pub struct PublishArgs {
    /// Who is writing: `human`, `agent`, or `system`.
    #[arg(long, value_parser = parse_actor_word)]
    actor: String,
    #[arg(long, value_parser = non_blank)]
    source: String,
    #[arg(long = "type")]
    kind: String,
    #[arg(long)]
    payload: String,
    /// The id of the event this one follows from.
    #[arg(long)]
    causation: Option<String>,
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

/// Appends one event built from `args` to `log`. `store` owns the
/// decode, so the CLI only parses flags. `root` is the writer's project
/// root, resolved once in `main`; `args.source` only names the writer,
/// so `publish` pairs the two into the `Source` the event carries.
/// `checkout` is the working tree a `file.cited` payload with no
/// `excerpt` reads from - in a worktree it differs from `root`, which
/// stays the project identity the event's `Source` carries.
/// Appends one event and prints its id, so a writer can cite it as the
/// `--causation` of the next. A cause the log lacks is an error: a typo
/// in provenance is worse than none.
pub fn publish(
    args: PublishArgs,
    log: &dyn EventLog,
    root: &Path,
    checkout: &Path,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let causation_id = args
        .causation
        .as_deref()
        .map(|id| known_event_id(id, log))
        .transpose()?;
    let source = crate::core::Source {
        name: args.source,
        path: root.to_path_buf(),
    };

    let event = if args.kind == "file.cited" {
        let payload = file_cited_payload(&args.payload, checkout)?;
        Event::new(
            store::parse_actor(&args.actor, me)?,
            source,
            causation_id,
            payload,
        )
    } else {
        let payload = serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
        let event = store::decode(&args.actor, source, &args.kind, causation_id, payload, me)?;
        // A raw map event would skip `Map::apply`, and one that breaks
        // a rule fails every fold from then on, with no undo in an
        // append-only log.
        if crate::core::map_of(event.payload()).is_some() {
            return Err(format!(
                "{} is written through `percept maps`, not published raw",
                args.kind
            )
            .into());
        }
        event
    };
    log.append(&event)?;
    print_lines(std::iter::once(event.id().as_uuid().to_string()))
}

/// The `payload` argument to `percept events publish --type file.cited`,
/// as the caller wrote it - `excerpt` absent when the tree should
/// supply it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFileCited {
    path: String,
    lines: Option<String>,
    excerpt: Option<String>,
}

/// Parses the JSON `publish --type file.cited` takes and builds its
/// payload - `build_file_cited` does the work, over a `Workspace`
/// opened once for this one call.
fn file_cited_payload(raw: &str, checkout: &Path) -> Result<Payload, Box<dyn std::error::Error>> {
    let raw: RawFileCited = serde_json::from_str(raw).map_err(store::Error::BadPayload)?;
    let lines = raw.lines.as_deref().map(store::parse_lines).transpose()?;
    let workspace = workspace::Workspace::new(checkout)?;
    build_file_cited(&workspace, &raw.path, lines, raw.excerpt)
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

/// Prints every map percept knows with its size: the log's maps, folded
/// from one read of `log` at `project`'s path, or at every path with
/// `--all-paths`.
pub fn maps_list(
    args: ListMapsArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    project: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = log.load()?;
    per_path(args.all_paths, args.json, project, &events, |path| {
        let maps = schemas.fold_all(mapstore::of_path(&events, path))?;
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
    schemas: &Schemas,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = log.load()?;
    per_path(args.all_paths, args.json, root, &events, |path| {
        print_map(mapstore::fold_map_at(schemas, &args.map, &events, path)?, &args)
    })
}

/// `percept start`: loads the log, folds every schema at `root`'s path,
/// and prints `mapstore::start`'s render, cut since the last session
/// anyone started here. Appends nothing: a look, not a checkpoint.
pub fn start(
    log: &dyn EventLog,
    schemas: &Schemas,
    root: &Path,
    checkout: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let events = log.load()?;
    let maps = schemas.fold_all(mapstore::of_path(&events, root))?;
    let since = mapstore::last_session(mapstore::of_path(&events, root));
    print_text(&mapstore::start(&maps, &events, root, checkout, since))
}

/// Prints the map `args.map` names' capabilities from its schema alone:
/// what it can hold and how to write to it. Needs no log and no fold,
/// so it works on an empty map.
pub fn maps_describe(args: DescribeMapArgs, schemas: &Schemas) -> Result<(), Box<dyn std::error::Error>> {
    let schema = schemas.find(&args.map)?;
    print_text(&mapstore::describe(&schema))
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
/// (`human` by default) with no cause, all under `mapstore::commit`'s
/// one lock. `me` resolves `human`/`user` to this log's own `HumanId`.
/// Returns the payload, for `add-node` to print the minted id.
fn write(
    target: MapArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let MapArgs {
        map,
        source: cited,
        actor,
    } = target;
    let actor = store::parse_actor(&actor, me)?;
    let event = mapstore::commit(log, schemas, &map, source, &cited, actor, mutation)?;
    Ok(event.payload().clone())
}

/// Adds a node to a map and prints its minted id, so a shell script can
/// capture it.
pub fn maps_add_node(
    args: AddNodeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = write(args.target, log, schemas, source, me, |sources| {
        Mutation::AddNode {
            kind: args.kind,
            name: args.name,
            properties: args.prop.into_iter().collect::<BTreeMap<_, _>>(),
            sources,
        }
    })?;
    if let Payload::NodeAdded { node, .. } = &payload {
        println!("{}", node.as_uuid());
    }
    Ok(())
}

/// Adds an edge between two nodes already in a map. `--from` and `--to`
/// are resolved against one fold of the map, taken before the write's
/// own atomic commit re-folds it.
pub fn maps_add_edge(
    args: EdgeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let map = mapstore::fold_map(log, schemas, &args.target.map, &source.path)?;
    let from = resolve_ref(&map, &args.from)?;
    let to = resolve_ref(&map, &args.to)?;
    write(args.target, log, schemas, source, me, |sources| {
        Mutation::AddEdge {
            kind: args.kind,
            from,
            to,
            sources,
        }
    })
    .map(drop)
}

/// Removes a node from a map, dropping the edges that touch it.
pub fn maps_remove_node(
    args: RemoveNodeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let map = mapstore::fold_map(log, schemas, &args.target.map, &source.path)?;
    let node = resolve_ref(&map, &args.node)?;
    write(args.target, log, schemas, source, me, |sources| {
        Mutation::RemoveNode {
            node,
            why: args.why,
            sources,
        }
    })
    .map(drop)
}

/// Removes an edge from a map.
pub fn maps_remove_edge(
    args: RemoveEdgeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let RemoveEdgeArgs { edge, why } = args;
    let map = mapstore::fold_map(log, schemas, &edge.target.map, &source.path)?;
    let from = resolve_ref(&map, &edge.from)?;
    let to = resolve_ref(&map, &edge.to)?;
    write(edge.target, log, schemas, source, me, |sources| {
        Mutation::RemoveEdge {
            kind: edge.kind,
            from,
            to,
            sources,
            why,
        }
    })
    .map(drop)
}

/// Changes a node already in a map - a rename, a property, or a
/// `why`-only comment - and prints the event id, the way `maps
/// add-node` prints the node it minted.
pub fn maps_change_node(
    args: ChangeNodeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let ChangeNodeArgs {
        target,
        node,
        name,
        prop,
        why,
    } = args;
    let map = mapstore::fold_map(log, schemas, &target.map, &source.path)?;
    let node = resolve_ref(&map, &node)?;
    let payload = write(target, log, schemas, source, me, |sources| {
        Mutation::ChangeNode {
            node,
            name,
            properties: prop.into_iter().collect::<BTreeMap<_, _>>(),
            sources,
            why,
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
/// "<value>"` is a rename, only meaningful under a change; `why
/// "<value>"` under a change is the change's own why rather than a
/// property, but stays a property under a fresh node), an `<edge kind>
/// <ref>`, or a `cites <path>[:<from>-<to>]` - until the next node line
/// or the document's end. A blank line is ignored; anything else names
/// its line number.
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

/// Adds every node and edge a document on stdin declares to `args.map`,
/// publishing a `file.cited` event for each `cites` line and folding its
/// id into that node's sources. Reads the document, then hands it to
/// `record_document`, which does the work `maps record`'s tests reach
/// directly, without stdin between them.
pub fn maps_record(
    args: RecordArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    checkout: &Path,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut document = String::new();
    io::stdin().read_to_string(&mut document)?;
    record_document(&document, args, log, schemas, source, checkout, me)
}

/// `maps_record`'s work, given the document text rather than reading it
/// from stdin. Every node and edge is checked and applied to one
/// in-memory fold of `args.map` - `Map::apply` enforces known kinds,
/// required properties, duplicate names, and known edge kinds, so this
/// only resolves an edge's ref, live, against the map as it stands at
/// that line: a bare kind name to the last node of that kind this
/// document declared above it, anything else through
/// `Map::resolve_str`. Nothing is appended until every node and edge
/// has passed, in one batch under the log's lock, so a failure midway,
/// whether a duplicate name, a missing `--source`, or a bad ref, leaves
/// nothing written; the error names the node or line it reached.
fn record_document(
    document: &str,
    args: RecordArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    checkout: &Path,
    me: Option<crate::core::HumanId>,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = parse_document(document)?;
    let total = nodes.len();

    let node_sources: Vec<EventId> = args
        .source
        .iter()
        .map(|id| known_event_id(id, log))
        .collect::<Result<_, _>>()?;
    let causation_id = args
        .causation
        .as_deref()
        .map(|id| known_event_id(id, log))
        .transpose()?;
    // Only opened when the document has a `cites` line to resolve - a
    // document with none should still record against a `checkout` that
    // does not exist, the way it always could.
    let workspace = if nodes.iter().any(|node| !node.cites.is_empty()) {
        Some(workspace::Workspace::new(checkout)?)
    } else {
        None
    };

    let RecordArgs { map, actor, .. } = args;
    let actor = store::parse_actor(&actor, me)?;
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

            // Either arm yields the mutation and the kind and name the
            // node has once it lands, so one tail applies both.
            let (mutation, kind, name) = if node.is_change {
                let target = resolve_node(snapshot.map(), &node.kind).map_err(context)?;
                let (kind, old_name) = (target.kind.clone(), target.name.clone());
                let mut properties = node.properties;
                let rename = properties.remove("name");
                let why = properties.remove("why");
                let name = rename.clone().unwrap_or_else(|| old_name.clone());
                let mutation = Mutation::ChangeNode {
                    node: NodeRef {
                        kind: kind.clone(),
                        name: old_name,
                    },
                    name: rename,
                    properties,
                    sources,
                    why,
                };
                (mutation, kind, name)
            } else {
                let mutation = Mutation::AddNode {
                    kind: node.kind.clone(),
                    name: node.name.clone(),
                    properties: node.properties,
                    sources,
                };
                (mutation, node.kind, node.name)
            };
            let payload = snapshot.apply(mutation, actor).map_err(|err| context(err.into()))?;
            let node_id = match &payload {
                Payload::NodeAdded { node, .. } | Payload::NodeChanged { node, .. } => *node,
                _ => unreachable!("AddNode and ChangeNode yield a node payload"),
            };
            batch.push(Event::new(actor, batch_source.clone(), None, payload));
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
                batch.push(Event::new(actor, batch_source.clone(), None, payload));
            }
        }

        Ok(batch)
    })?;

    let map = mapstore::fold_map(log, schemas, &map, &source.path)?;
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
                node_lines.push(format!("{short_id} {kind} {}", quote(name)));
            }
            Payload::NodeChanged { node, name, .. } => {
                let short_id = map.short_id(*node).unwrap_or_default();
                match name {
                    Some(name) => node_lines.push(format!("{short_id} changed to {}", quote(name))),
                    None => node_lines.push(format!("{short_id} changed")),
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
        .map(|s| parse_time("since", s))
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(|s| parse_time("until", s))
        .transpose()?;

    // An inverted window can never match, whatever the log holds -
    // `--since 1h --until 2h` is how "between one and two hours ago"
    // is mistyped. Rejecting it keeps an empty result meaning the log
    // has nothing, the same guarantee the filters above give.
    if let (Some(since), Some(until)) = (since, until) {
        if since >= until {
            return Err(format!("--since {since} is not before --until {until}"));
        }
    }

    Ok(EventQuery {
        since,
        until,
        actors,
        sources: args.source.clone(),
        kinds,
        text: args.contains.clone(),
        size: args.size,
    })
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

/// Parses a `--since`/`--until` value: an ISO-8601 timestamp, or a
/// relative shorthand - `<N>d`, `<N>h`, `<N>m` - measured back from now.
/// `flag` names the flag the value came from, so a rejected value's
/// error says which one.
fn parse_time(flag: &str, s: &str) -> Result<Timestamp, String> {
    store::parse_time(s).map_err(|_| format!("invalid --{flag} value {s}"))
}

pub mod hook;
pub mod init;

#[cfg(test)]
mod tests;
