//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events search` queries the log,
//! `percept events show` dereferences one event by id, `percept maps`
//! folds a cognitive map from the log and prints it - except `code`,
//! walked fresh from the working tree - `percept ask` runs one full
//! turn - including the tool loop - and prints the reply, `percept
//! reflect` runs one asking the model to revise its maps, `percept
//! hook <client>` records one coding client's turn from the hook JSON
//! it reads on stdin - see `hook` - and `percept init <client>` writes
//! that client's project config so its hooks call `percept hook
//! <client>` - see `init`. A
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

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::Path;

use clap::{Args, Parser, Subcommand};
use tokio_stream::StreamExt;

use crate::app::{run_tool, AppService, ToolStep};
use crate::code;
use crate::core::{
    Actor, EventId, EventLog, EventQuery, EventSearch, Map, Mutation, NodeRef, Payload,
    Schemas,
};
use crate::harness::Chunk;
use crate::mapstore;
use crate::shared::Timestamp;
use crate::store;
use crate::tools;

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

Run with no arguments to open the TUI. Every subcommand reaches the log \
without it: `events publish` appends one event, `events search` and \
`events show` query it, `maps list` and `maps show` print a cognitive \
map folded from it - `code`, the map of files and imports, is walked \
fresh from the working tree instead - `ask` runs one full turn and \
prints the reply, `reflect` runs one turn asking the model to \
revise its maps, `hook <client>` records one coding client's turn \
from the hook JSON it reads on stdin, and `init <client>` writes that \
client's project config to call it.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Work with the event log directly, bypassing the TUI.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Read percept's maps - the cognitive ones folded from the log,
    /// and `code`, walked fresh from the working tree.
    Maps {
        #[command(subcommand)]
        command: MapsCommand,
    },
    /// Run one turn headlessly and print the reply.
    Ask(AskArgs),
    /// Run one turn asking the model to revise its maps from the log.
    Reflect,
    /// Record one coding client's turn from the hook JSON it sends on
    /// stdin. Never fails the client's turn: an error prints to
    /// stderr and still exits with a JSON object on stdout.
    Hook(hook::HookArgs),
    /// Write a coding client's project config so its hooks call
    /// `percept hook <client>`.
    Init(init::InitArgs),
}

#[derive(Subcommand)]
pub enum MapsCommand {
    /// Every map with its node and edge counts, one JSON object per line.
    List(ListMapsArgs),
    /// One map's nodes, then its edges, one JSON object per line.
    Show(ShowMapArgs),
    /// Add a node to a map. Prints the minted node id.
    AddNode(AddNodeArgs),
    /// Add an edge between two nodes already in a map.
    AddEdge(EdgeArgs),
    /// Remove a node from a map, dropping the edges that touch it.
    RemoveNode(RemoveNodeArgs),
    /// Remove an edge from a map.
    RemoveEdge(EdgeArgs),
}

/// How `maps show` and `maps list` print a map.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// One JSON object per line - map, then node, then edge.
    #[default]
    Json,
    /// The rendered Markdown, as written to `.percept/<map>.md`.
    #[value(alias = "markdown")]
    Md,
}

#[derive(Args)]
pub struct ShowMapArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
    /// `json` (default) for one JSON object per line, `md` for the
    /// rendered Markdown.
    #[arg(long, value_enum, default_value_t = Format::Json)]
    format: Format,
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
    /// added since, and the ends of the edges added since. Refused for
    /// the code map, which is walked fresh and has no history.
    #[arg(long, value_parser = |s: &str| parse_time("since", s))]
    since: Option<Timestamp>,
    /// Fold every project's events instead of only this one's. Ignored
    /// for the code map, which is never folded from the log.
    #[arg(long)]
    all_projects: bool,
}

impl ShowMapArgs {
    /// Whether this names the code map - derived from the working tree,
    /// so dispatch never opens the log to find out.
    pub fn is_code(&self) -> bool {
        self.map == crate::core::CODE
    }
}

#[derive(Args)]
pub struct ListMapsArgs {
    /// Fold every project's events instead of only this one's.
    #[arg(long)]
    all_projects: bool,
    /// `json` (default) for one JSON object per line, `md` for a
    /// Markdown table.
    #[arg(long, value_enum, default_value_t = Format::Json)]
    format: Format,
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
    /// Who is writing: `user` for a human at the terminal, `model` for an
    /// agent recording on their behalf. The map shows the difference.
    #[arg(long, default_value = "user", value_parser = parse_actor_arg)]
    actor: Actor,
}

fn parse_actor_arg(s: &str) -> Result<Actor, String> {
    store::parse_actor(s).map_err(|err| err.to_string())
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
    reason: String,
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
    #[arg(long)]
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
    #[arg(long)]
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

#[derive(Args)]
pub struct AskArgs {
    /// The prompt to send.
    #[arg(value_parser = non_blank)]
    pub prompt: String,
    /// Run every tool call the policy would ask about. Headless, there
    /// is no one to ask, so without this such a call is declined.
    #[arg(long)]
    pub yes: bool,
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
    let id = map.resolve_str(s)?;
    let node = map.node(id).expect("resolve_str returns a live node's id");
    Ok(NodeRef {
        kind: node.kind.clone(),
        name: node.name.clone(),
    })
}

/// Appends one event built from `args` to `log`. `store` owns the
/// decode, so the CLI only parses flags. `root` is the writer's project
/// root, resolved once in `main`; `args.source` only names the writer,
/// so `publish` pairs the two into the `Source` the event carries.
/// Appends one event and prints its id, so a writer can cite it as the
/// `--causation` of the next. A cause the log lacks is an error: a typo
/// in provenance is worse than none.
pub fn publish(
    args: PublishArgs,
    log: &dyn EventLog,
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let causation_id = args
        .causation
        .as_deref()
        .map(|id| known_event_id(id, log))
        .transpose()?;
    let payload = serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
    let source = crate::core::Source {
        name: args.source,
        path: root.to_path_buf(),
    };
    let event = store::decode(&args.actor, source, &args.kind, causation_id, payload)?;
    // A raw map event would skip `Map::apply`, and one that breaks a
    // rule fails every fold from then on, with no undo in an
    // append-only log.
    if crate::core::map_of(event.payload()).is_some() {
        return Err(format!(
            "{} is written through `percept maps`, not published raw",
            args.kind
        )
        .into());
    }
    log.append(&event)?;
    print_lines(std::iter::once(event.id().as_uuid().to_string()))
}

fn known_event_id(
    id: &str,
    log: &dyn EventLog,
) -> Result<crate::core::EventId, Box<dyn std::error::Error>> {
    let parsed = store::parse_event_id(id)?;
    if log.get(parsed)?.is_none() {
        return Err(format!("no event with id {id}").into());
    }
    Ok(parsed)
}

/// Searches `log` for events matching `args`, printing one JSON object
/// per line in log order. `store` owns the wire shape; the CLI only
/// builds the query and formats the result.
pub fn search(args: SearchArgs, log: &dyn EventSearch) -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query(&args)?;
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

/// The scope `maps list` and `maps show` fold: every project's events
/// with `--all-projects`, else only `root`'s.
fn scope(all_projects: bool, root: &Path) -> crate::core::Scope {
    if all_projects {
        crate::core::Scope::All
    } else {
        crate::core::Scope::Project(root.to_path_buf())
    }
}

/// Prints every map percept knows with its size: the log's maps, folded
/// from one read of `log` and scoped to `project` unless `args` says
/// otherwise, then the code map, walked fresh from `tree` - the
/// checkout, which in a worktree is not the project's path.
pub fn maps_list(
    args: ListMapsArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    project: &Path,
    tree: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut maps = schemas.fold_all(&scope(args.all_projects, project), &log.load()?)?;
    maps.push(code::build(tree)?);
    match args.format {
        Format::Json => print_lines(maps.iter().map(mapstore::encode_map)),
        Format::Md => print_text(&mapstore::catalogue(&maps)),
    }
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
    let map = mapstore::fold_map(log, schemas, &args.map, &scope(args.all_projects, root))?;
    print_map(map, &args)
}

/// Prints the code map, walked fresh from `root` - never the log, so
/// this runs in a directory with no `percept.jsonl`. `print_map`'s
/// `select` refuses `--since`: every node is as old as this walk.
pub fn maps_show_code(args: ShowMapArgs, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let map = code::build(root)?;
    print_map(map, &args)
}

/// `maps_show` and `maps_show_code`'s shared tail: cut `map` to
/// `args`'s filters, then print it nodes-then-edges. `--since` runs
/// after `--around`, so it reads as "what changed near this node".
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
    match args.format {
        Format::Json => print_lines(mapstore::encode_lines(fragment.map())),
        Format::Md => print_text(&mapstore::markdown(fragment.map())),
    }
}

/// One map change from the shell: `target`'s cited events resolved and
/// `mutation` checked, applied, and committed as actor `user` with no
/// cause, all under `mapstore::commit`'s one lock. Returns the
/// payload, for `add-node` to print the minted id.
fn write(
    target: MapArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
    mutation: impl FnOnce(Vec<EventId>) -> Mutation,
) -> Result<Payload, Box<dyn std::error::Error>> {
    let MapArgs {
        map,
        source: cited,
        actor,
    } = target;
    let scope = source.scope();
    let event = mapstore::commit(log, schemas, &map, &scope, source, &cited, actor, mutation)?;
    Ok(event.payload().clone())
}

/// Adds a node to a map and prints its minted id, so a shell script can
/// capture it.
pub fn maps_add_node(
    args: AddNodeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
) -> Result<(), Box<dyn std::error::Error>> {
    let payload = write(args.target, log, schemas, source, |sources| {
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
) -> Result<(), Box<dyn std::error::Error>> {
    let scope = source.scope();
    let map = mapstore::fold_map(log, schemas, &args.target.map, &scope)?;
    let from = resolve_ref(&map, &args.from)?;
    let to = resolve_ref(&map, &args.to)?;
    write(args.target, log, schemas, source, |sources| {
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
) -> Result<(), Box<dyn std::error::Error>> {
    let scope = source.scope();
    let map = mapstore::fold_map(log, schemas, &args.target.map, &scope)?;
    let node = resolve_ref(&map, &args.node)?;
    write(args.target, log, schemas, source, |sources| {
        Mutation::RemoveNode {
            node,
            reason: args.reason,
            sources,
        }
    })
    .map(drop)
}

/// Removes an edge from a map.
pub fn maps_remove_edge(
    args: EdgeArgs,
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &crate::core::Source,
) -> Result<(), Box<dyn std::error::Error>> {
    let scope = source.scope();
    let map = mapstore::fold_map(log, schemas, &args.target.map, &scope)?;
    let from = resolve_ref(&map, &args.from)?;
    let to = resolve_ref(&map, &args.to)?;
    write(args.target, log, schemas, source, |sources| {
        Mutation::RemoveEdge {
            kind: args.kind,
            from,
            to,
            sources,
        }
    })
    .map(drop)
}

/// A reader that stops early - `head`, or a `jq` that has seen enough -
/// closes the pipe. That is the caller's choice, not a failure to
/// report.
fn stop_if_pipe_closed(e: io::Error) -> Result<(), Box<dyn std::error::Error>> {
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
fn parse_query(args: &SearchArgs) -> Result<EventQuery, String> {
    let kinds = args
        .kind
        .iter()
        .map(|kind| store::parse_kind(kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let actors = args
        .actor
        .iter()
        .map(|actor| store::parse_actor(actor).map_err(|e| e.to_string()))
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
    println!("{}", tools::read(log, &args.id, start, end)?);
    Ok(())
}

/// Runs one turn on `app` - submitting `prompt` as `actor`, then
/// draining the reply stream chunk by chunk - and prints the reply to
/// stdout. No channel, no spawned task: unlike the TUI, nothing else
/// needs the thread while headless, so a tool runs inline and the turn
/// is one plain `await` loop. Each tool call and its result print to
/// stderr as they happen, so stdout stays pipeable. That trace is for
/// watching a run live; the log is what a run is read back from. `yes`
/// is the user's standing answer to every call the policy puts to
/// them - what `y` is in the TUI; without it, headless, such a call is
/// declined.
pub async fn run_turn(
    mut app: Box<dyn AppService>,
    actor: Actor,
    prompt: String,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = app.submit_as(actor, prompt)?;
    // What stdout gets. `App` clears its own reply buffer at each tool
    // call and again when the cap ends a turn, so a turn that spoke
    // before calling a tool would otherwise print only its last leg.
    let mut reply = String::new();

    loop {
        match stream.next().await {
            // Each arm echoes for itself: `App` decides what a call
            // means, and a call it refused never happened.
            Some(Ok(Chunk::ToolCall { tool, arguments })) => {
                stream = match app.begin_tool(&tool, arguments.clone())? {
                    ToolStep::Ask(_, arguments) if !yes => {
                        eprintln!("⚒ {tool}({arguments}) - declined: needs approval, run in the TUI or pass --yes");
                        app.decline_tool()?
                    }
                    ToolStep::Run(run, arguments) | ToolStep::Ask(run, arguments) => {
                        eprintln!("⚒ {tool}({arguments})");
                        let output = run_tool(&*run, &arguments);
                        eprintln!("⚒ {}", output.content);
                        app.finish_tool(output)?
                    }
                    ToolStep::Continue(stream) => {
                        eprintln!("⚒ {tool}({arguments}) - no such tool");
                        stream
                    }
                    ToolStep::Stop => break,
                };
            }
            Some(Ok(chunk)) => {
                if let Chunk::Reply(text) = &chunk {
                    reply.push_str(text);
                }
                app.append_chunk(chunk);
            }
            // A failed reply is shown, never logged - the stream's own
            // words are this run's error. Whatever text arrived before
            // it still commits, and still prints: the words reached the
            // log, so stdout is not the surface that should lose them.
            Some(Err(err)) => {
                app.end_stream()?;
                print_reply(&reply)?;
                return Err(err.to_string().into());
            }
            None => break,
        }
    }

    app.end_stream()?;
    print_reply(&reply)
}

/// Writes the reply to stdout, saying nothing when the turn produced no
/// text. A reader that stops early is the caller's choice, not a
/// failure - the same courtesy `search` extends.
fn print_reply(reply: &str) -> Result<(), Box<dyn std::error::Error>> {
    if reply.is_empty() {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    writeln!(out, "{reply}").or_else(stop_if_pipe_closed)
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
