//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events search` queries the log,
//! `percept events show` dereferences one event by id, `percept maps`
//! folds a cognitive map from the log and prints it - except `code`,
//! walked fresh from the working tree - `percept ask` runs one full
//! turn - including the tool loop - and prints the reply, and `percept
//! reflect` runs one asking the model to revise its maps. A
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
use crate::percept::{
    self, Actor, Chunk, Event, EventLog, EventQuery, EventSearch, Map, Mutation, NodeRef, Payload,
};
use crate::shared::Timestamp;
use crate::store;

#[derive(Parser)]
#[command(name = "percept")]
#[command(about = "Record what happens across your tools, so a model can query it.")]
#[command(long_about = "\
Record what happens across your tools, so a model can query it.

percept keeps an append-only log of events - prompts, replies, and tool \
calls - in percept.jsonl in the working directory. It never ranks, \
summarises, or answers: its job is to make looking cheap and leave \
relevance to the caller.

Run with no arguments to open the TUI. Every subcommand reaches the log \
without it: `events publish` appends one event, `events search` and \
`events show` query it, `maps list` and `maps show` print a cognitive \
map folded from it - `code`, the map of files and imports, is walked \
fresh from the working tree instead - `ask` runs one full turn and \
prints the reply, and `reflect` runs one turn asking the model to \
revise its maps.")]
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
}

#[derive(Subcommand)]
pub enum MapsCommand {
    /// Every map with its node and edge counts, one JSON object per line.
    List,
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

#[derive(Args)]
pub struct ShowMapArgs {
    /// The map's name, as `maps list` prints it.
    map: String,
    /// Repeatable. Keep only nodes of any of these kinds, and the edges
    /// between them.
    #[arg(long)]
    kind: Vec<String>,
    /// `kind:name` of a node. Keep only it and its neighbourhood,
    /// reached along edges in either direction.
    #[arg(long, value_parser = parse_node_ref)]
    around: Option<NodeRef>,
    /// How many edges out `--around` reaches; 0 is the node alone.
    #[arg(long, default_value_t = 1, requires = "around")]
    depth: usize,
}

impl ShowMapArgs {
    /// Whether this names the code map - derived from the working tree,
    /// so dispatch never opens the log to find out.
    pub fn is_code(&self) -> bool {
        self.map == percept::CODE.name
    }
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
    #[arg(long)]
    kind: String,
    #[arg(long)]
    name: String,
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
    /// `kind:name` of the node the edge starts at.
    #[arg(long, value_parser = parse_node_ref)]
    from: NodeRef,
    /// `kind:name` of the node the edge points to.
    #[arg(long, value_parser = parse_node_ref)]
    to: NodeRef,
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
fn non_blank(s: &str) -> Result<String, String> {
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

/// Parses `kind:name` for `--from`/`--to`, split on the first `:` so a
/// name may carry one itself.
fn parse_node_ref(s: &str) -> Result<NodeRef, String> {
    let (kind, name) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid {s:?}, expected kind:name"))?;
    Ok(NodeRef {
        kind: non_blank(kind)?,
        name: non_blank(name)?,
    })
}

/// Appends one event built from `args` to `log`. `store` owns the
/// decode, so the CLI only parses flags.
pub fn publish(args: PublishArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
    let event = store::decode(&args.actor, args.source, &args.kind, payload)?;
    // A raw map event would skip `Map::apply`, and one that breaks a
    // rule fails every fold from then on, with no undo in an
    // append-only log.
    if percept::map_of(event.payload()).is_some() {
        return Err(format!(
            "{} is written through `percept maps`, not published raw",
            args.kind
        )
        .into());
    }
    log.append(&event)
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

/// Prints every map percept knows with its size: the log's maps, folded
/// from one read of `log`, then the code map, walked fresh from the
/// working directory.
pub fn maps_list(log: &dyn EventLog, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut maps = Map::fold_all(&log.load()?)?;
    maps.push(code::build(root)?);
    print_lines(maps.iter().map(store::encode_map))
}

/// Prints the map `args.map` names, nodes then edges. `--around` cuts
/// it to a neighbourhood first, then `--kind` cuts that to its kinds,
/// so a node of another kind still counts as a step on the way.
pub fn maps_show(args: ShowMapArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let map = store::fold_map(log, &args.map)?;
    print_map(map, &args)
}

/// Prints the code map, walked fresh from `root` - never the log, so
/// this runs in a directory with no `percept.jsonl`.
pub fn maps_show_code(args: ShowMapArgs, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let map = code::build(root)?;
    print_map(map, &args)
}

/// `maps_show` and `maps_show_code`'s shared tail: cut `map` to
/// `args`'s filters, then print it nodes-then-edges.
fn print_map(mut map: Map, args: &ShowMapArgs) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(node) = &args.around {
        map = map.around(node, args.depth)?;
    }
    if !args.kind.is_empty() {
        map = map.keep_kinds(&args.kind)?;
    }
    let nodes = map.nodes().iter().map(store::encode_node);
    let edges = map.edges().iter().map(store::encode_edge);
    print_lines(nodes.chain(edges))
}

/// Commits a shell user's map change: actor `user`, source `cli`, no
/// cause.
fn record(log: &dyn EventLog, payload: Payload) -> Result<(), Box<dyn std::error::Error>> {
    log.append(&Event::new(Actor::User, "cli".to_string(), None, payload))
}

/// Adds a node to a map and prints its minted id, so a shell script can
/// capture it.
pub fn maps_add_node(
    args: AddNodeArgs,
    log: &dyn EventLog,
) -> Result<(), Box<dyn std::error::Error>> {
    let MapArgs { map, source } = args.target;
    let payload = store::revise(log, &map, &source, |sources| Mutation::AddNode {
        kind: args.kind,
        name: args.name,
        properties: args.prop.into_iter().collect::<BTreeMap<_, _>>(),
        sources,
    })?;
    if let Payload::NodeAdded { node, .. } = &payload {
        println!("{}", node.as_uuid());
    }
    record(log, payload)
}

/// Adds an edge between two nodes already in a map.
pub fn maps_add_edge(args: EdgeArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let MapArgs { map, source } = args.target;
    let payload = store::revise(log, &map, &source, |sources| Mutation::AddEdge {
        kind: args.kind,
        from: args.from,
        to: args.to,
        sources,
    })?;
    record(log, payload)
}

/// Removes a node from a map, dropping the edges that touch it.
pub fn maps_remove_node(
    args: RemoveNodeArgs,
    log: &dyn EventLog,
) -> Result<(), Box<dyn std::error::Error>> {
    let MapArgs { map, source } = args.target;
    let payload = store::revise(log, &map, &source, |sources| Mutation::RemoveNode {
        node: NodeRef {
            kind: args.kind,
            name: args.name,
        },
        reason: args.reason,
        sources,
    })?;
    record(log, payload)
}

/// Removes an edge from a map.
pub fn maps_remove_edge(
    args: EdgeArgs,
    log: &dyn EventLog,
) -> Result<(), Box<dyn std::error::Error>> {
    let MapArgs { map, source } = args.target;
    let payload = store::revise(log, &map, &source, |sources| Mutation::RemoveEdge {
        kind: args.kind,
        from: args.from,
        to: args.to,
        sources,
    })?;
    record(log, payload)
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
    println!("{}", store::read(log, &args.id, start, end)?);
    Ok(())
}

/// Runs one turn on `app` - submitting `prompt` as `actor`, then
/// draining the reply stream chunk by chunk - and prints the reply to
/// stdout. No channel, no spawned task: unlike the TUI, nothing else
/// needs the thread while headless, so a tool runs inline and the turn
/// is one plain `await` loop. Each tool call and its result print to
/// stderr as they happen, so stdout stays pipeable. That trace is for
/// watching a run live; the log is what a run is read back from.
pub async fn run_turn(
    mut app: Box<dyn AppService>,
    actor: Actor,
    prompt: String,
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
                    ToolStep::Run(run, arguments) => {
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
    let parsed = match relative_minutes(s) {
        Some(minutes) => Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    };
    parsed.ok_or_else(|| format!("invalid --{flag} value {s}"))
}

/// `<N>d`, `<N>h`, or `<N>m` as a count of minutes. `None` for anything
/// else - `parse_time` then tries it as ISO-8601.
fn relative_minutes(s: &str) -> Option<i64> {
    let (digits, unit) = s.split_at_checked(s.len().checked_sub(1)?)?;
    let n: i64 = digits.parse().ok()?;

    match unit {
        "d" => n.checked_mul(24 * 60),
        "h" => n.checked_mul(60),
        "m" => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
