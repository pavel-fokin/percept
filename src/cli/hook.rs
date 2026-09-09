//! `percept hook <client>` - reads one hook JSON object off stdin, the
//! shape Claude Code and Codex both send, and turns it into events on
//! the same log every other subcommand appends to. Never fails the
//! client's turn: `main` catches every error here, prints it to
//! stderr as `percept hook: <error>`, and still prints `{}` to
//! stdout.
//!
//! Reading and validating the input (`read`) is split from acting on
//! it (`run`): `main` needs the client's own `cwd` before it can find
//! the checkout and open the log, so the input is parsed first, and
//! everything else only afterwards.
//!
//! A turn's events cite one another through a per-turn `store::TurnState`
//! kept beside the log, under `hook-sessions/<root>` - `root`'s slashes
//! replaced by `%`, so one project's turns never collide with
//! another's: the id of the turn's prompt event, so a later tool call
//! or reply can name it as its cause. Opening it also takes its lock,
//! held exclusively for the length of one hook call, so two hook calls
//! for the same turn never race.

use std::fs::File;
use std::io::{BufRead, Read};
use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{Actor, Event, EventId, EventLog, Map, Node, Payload, Schemas, Source};
use crate::shared::Timestamp;
use crate::store::TurnState;

/// `percept hook <client>` - `client` names the writer whose turn this
/// is, and becomes every event's source.
#[derive(clap::Args)]
pub struct HookArgs {
    #[arg(value_parser = crate::cli::non_blank)]
    pub client: String,
}

/// One hook JSON object, the shape every client sends: `cwd`,
/// `session_id`, and an optional `turn_id` mean the same thing
/// whichever client sent them; `event` carries the fields specific to
/// `hook_event_name`.
#[derive(Deserialize)]
pub struct HookInput {
    cwd: String,
    session_id: String,
    #[serde(default)]
    turn_id: String,
    #[serde(flatten)]
    event: HookEvent,
}

impl HookInput {
    /// The client's own working directory - what `main` resolves the
    /// checkout and project root from, since the process's own cwd may
    /// be anywhere the client's shell happened to start it.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
}

/// The four hook events percept understands, tagged by
/// `hook_event_name`, each carrying only the fields `run` needs from
/// it.
#[derive(Deserialize)]
#[serde(tag = "hook_event_name")]
enum HookEvent {
    SessionStart {},
    UserPromptSubmit {
        prompt: String,
    },
    PostToolUse {
        tool_name: String,
        tool_input: Value,
        tool_response: Value,
    },
    Stop {
        last_assistant_message: Option<String>,
        transcript_path: Option<String>,
    },
}

/// Every event name `HookEvent` deserialises, in the order `init`
/// writes their config entries. `hook::tests` proves this list and
/// `HookEvent::name` cannot drift apart.
pub const EVENTS: [&str; 4] = ["SessionStart", "UserPromptSubmit", "PostToolUse", "Stop"];

impl HookEvent {
    /// Used only by `hook::tests`, to prove `EVENTS` and this match
    /// name every variant the same way.
    #[cfg(test)]
    fn name(&self) -> &'static str {
        match self {
            HookEvent::SessionStart { .. } => "SessionStart",
            HookEvent::UserPromptSubmit { .. } => "UserPromptSubmit",
            HookEvent::PostToolUse { .. } => "PostToolUse",
            HookEvent::Stop { .. } => "Stop",
        }
    }
}

/// Reads one hook JSON object from `input` in full and parses it.
/// Reading `input` to the end before returning, even on a parse error,
/// means a caller that runs this before doing anything else never
/// leaves the client's own pipe half read. `session_id` empty is the
/// one check serde's shape can't express; everything else - an unknown
/// `hook_event_name`, a missing or mistyped field - is its error.
pub fn read(input: &mut dyn Read) -> Result<HookInput, Box<dyn std::error::Error>> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    let input: HookInput = serde_json::from_str(&text)?;
    if input.session_id.is_empty() {
        return Err("session_id must not be empty".into());
    }
    Ok(input)
}

/// Appends the events `input`'s event implies to `log` under `source`,
/// and returns the JSON object the client expects back on stdout -
/// `{}` unless the event asks for something. `sessions_dir` holds one
/// directory per checkout root, created if missing. `checkout` is only
/// read - as schemas, from `.percept/schemas/*.toml` - for
/// `SessionStart`; a project schema that fails to load must not also
/// break the other three events, which need no schema at all.
pub fn run(
    input: HookInput,
    source: &Source,
    log: &dyn EventLog,
    sessions_dir: &Path,
    checkout: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let dir = sessions_dir.join(state_dir_name(&source.path));
    let name = state_file_name(&source.name, &input.session_id, &input.turn_id);
    let mut state = TurnState::open(&dir, &name)?;

    match input.event {
        HookEvent::SessionStart {} => {
            let schemas = crate::mapstore::load_schemas(checkout)?;
            start_session(source, log, &schemas)
        }
        HookEvent::UserPromptSubmit { prompt } => submit_prompt(prompt, source, log, &mut state),
        HookEvent::PostToolUse {
            tool_name,
            tool_input,
            tool_response,
        } => {
            let cause = state.cause()?;
            record_tool_use(tool_name, tool_input, tool_response, source, log, cause)
        }
        HookEvent::Stop {
            last_assistant_message,
            transcript_path,
        } => {
            let cause = state.cause()?;
            let output = record_stop(last_assistant_message, transcript_path, source, log, cause);
            let _ = state.remove();
            output
        }
    }
}

/// `SessionStart`: finds the previous `session.started` event this
/// source recorded against this project, if any - what a fragment cuts
/// the log to since - records a fresh one for the next call to find,
/// and folds every log-backed schema to report what each map gained
/// since then, what is still open on it, and one concrete next step.
/// What "gained" and "open" mean is read off `Schema` - `headline_kinds`
/// and `settlement` - never off a map's name, so a project's own
/// schema (an `ideas` map with no settlement, say) reports without any
/// code naming it.
fn start_session(
    source: &Source,
    log: &dyn EventLog,
    schemas: &Schemas,
) -> Result<Value, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let since = last_session(&events, source);

    log.append(&Event::session_started(source.clone()))?;

    let project = project_name(source);
    let header = match since {
        Some(at) => format!("percept · project {project}\nsince your last session here ({at})"),
        None => format!("percept · project {project}\nfirst session here"),
    };

    let maps = schemas.fold_all(&source.scope(), &events)?;

    let mut sections = vec![header];
    if let Some(at) = since {
        sections.push(gained_block(&maps, at));
    }
    let (open_blocks, pointer) = open_blocks_and_pointer(&maps);
    sections.extend(open_blocks);
    sections.extend(pointer);

    Ok(json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": sections.join("\n\n"),
        }
    }))
}

/// The short id `node` has on `map`, or a `kind:name` fallback for the
/// unexpected case a headline node carries none.
fn line_id(map: &Map, node: &Node) -> String {
    map.short_id(node.id)
        .unwrap_or_else(|| format!("{}:{}", node.kind, node.name))
}

/// How many lines of a gained or open list `gained_block` and
/// `open_blocks_and_pointer` show before folding the rest into a
/// trailing count.
const LIMIT: usize = 5;

/// Up to `LIMIT` of `items`, each turned into a line by `line`, with a
/// trailing `+N more` when there were more - the one truncation rule
/// both blocks below share.
fn capped_lines(items: &[&Node], line: impl Fn(&Node) -> String) -> Vec<String> {
    let mut lines: Vec<String> = items.iter().take(LIMIT).map(|node| line(node)).collect();
    if items.len() > LIMIT {
        lines.push(format!("+{} more", items.len() - LIMIT));
    }
    lines
}

/// What each folded map gained since `since`: a counts line for every
/// map, in fold order, then up to `LIMIT` lines per map that gained
/// anything - a node's `added_at` is compared directly, not
/// `Map::since`, which would also surface an older node a fresh edge
/// only touched.
fn gained_block(maps: &[Map], since: Timestamp) -> String {
    let per_map: Vec<Vec<&Node>> = maps
        .iter()
        .map(|map| map.headlines().filter(|node| node.added_at >= since).collect())
        .collect();

    let counts = maps
        .iter()
        .zip(&per_map)
        .map(|(map, gained)| format!("{} +{}", map.schema().name, gained.len()))
        .collect::<Vec<_>>()
        .join("   ");

    let mut lines = vec![counts];
    for (map, gained) in maps.iter().zip(&per_map) {
        if gained.is_empty() {
            continue;
        }
        lines.extend(capped_lines(gained, |node| {
            format!("{} {} {:?}", line_id(map, node), node.kind, node.name)
        }));
    }
    lines.join("\n")
}

/// One `open {of} (...)` block per settled map that has open items - a
/// map without a `Settlement` (an `ideas` map, say) is skipped entirely,
/// never by name, since `Map::open` is empty there - plus the fragment
/// pointer at the first open item found, walking maps in fold order.
fn open_blocks_and_pointer(maps: &[Map]) -> (Vec<String>, Option<String>) {
    let mut blocks = Vec::new();
    let mut pointer = None;

    for map in maps {
        let Some(settlement) = map.schema().settlement.as_ref() else {
            continue;
        };
        let open: Vec<&Node> = map.open().collect();
        if open.is_empty() {
            continue;
        }

        let total = open.len();
        let header = if total > LIMIT {
            format!("open {} ({total}, showing {LIMIT})", settlement.of)
        } else {
            format!("open {} ({total})", settlement.of)
        };
        let mut lines = vec![header];
        lines.extend(capped_lines(&open, |node| format!("{} {:?}", line_id(map, node), node.name)));
        blocks.push(lines.join("\n"));

        pointer.get_or_insert_with(|| {
            format!(
                "fragment: percept maps show {} --around {}",
                map.schema().name,
                line_id(map, open[0])
            )
        });
    }

    (blocks, pointer)
}

/// The latest `session.started` event this exact source (client name
/// and project path) recorded, if any - `None` on a project's first
/// session with this client.
fn last_session(events: &[Event], source: &Source) -> Option<Timestamp> {
    events
        .iter()
        .filter(|event| {
            matches!(event.payload(), Payload::SessionStarted)
                && event.source().name == source.name
                && event.source().path == source.path
        })
        .map(Event::created_at)
        .max()
}

/// `source.path`'s last component, the name a reader knows the project
/// by - falling back to the whole path on the rare root with none.
fn project_name(source: &Source) -> String {
    source
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.path.display().to_string())
}

/// `UserPromptSubmit`: clears the turn's previous cause before doing
/// anything else, so a prompt that then fails to commit never leaves a
/// later event citing the wrong one. Records the prompt as
/// `message.received` from `user`, stores its id as the turn's cause,
/// and returns the client's expected `additionalContext`.
fn submit_prompt(
    prompt: String,
    source: &Source,
    log: &dyn EventLog,
    state: &mut TurnState,
) -> Result<Value, Box<dyn std::error::Error>> {
    state.clear()?;

    let committed = Event::message_received(Actor::User, prompt, source.clone(), None);
    let id = committed.id();
    log.append(&committed)?;
    state.set(id)?;

    Ok(json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": format!("percept event {}", id.as_uuid()),
        }
    }))
}

/// `PostToolUse`: records the call, caused by the turn's prompt, then
/// its result, caused by the call.
fn record_tool_use(
    tool_name: String,
    tool_input: Value,
    tool_response: Value,
    source: &Source,
    log: &dyn EventLog,
    cause: Option<EventId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let arguments = serde_json::to_string(&tool_input)?;
    let response = match tool_response {
        Value::String(text) => text,
        other => serde_json::to_string(&other)?,
    };

    let call = Event::tool_called(tool_name, arguments, source.clone(), cause);
    let call_id = call.id();
    log.append(&call)?;
    let result = Event::tool_resulted(response, source.clone(), Some(call_id));
    log.append(&result)?;
    Ok(json!({}))
}

/// `Stop`: the turn's reply, `last_assistant_message` when given, else
/// read from the Claude transcript at `transcript_path`. A non-empty
/// reply is recorded as `message.received` from `model`, caused by the
/// turn's prompt.
fn record_stop(
    last_assistant_message: Option<String>,
    transcript_path: Option<String>,
    source: &Source,
    log: &dyn EventLog,
    cause: Option<EventId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let reply = match last_assistant_message {
        Some(text) => Some(text),
        None => match transcript_path {
            None => None,
            Some(text) if text.is_empty() => None,
            Some(path) => Some(claude_reply(Path::new(&path))?),
        },
    };

    if let Some(reply) = reply {
        if !reply.trim().is_empty() {
            let committed = Event::message_received(Actor::Model, reply, source.clone(), cause);
            log.append(&committed)?;
        }
    }
    Ok(json!({}))
}

/// The current turn's assistant text from a Claude transcript: text
/// blocks accumulate across `assistant` entries, and reset whenever a
/// `user` entry starts a new turn - one whose content is not itself a
/// tool result, which is how a user's own message is told apart from
/// the tool results Claude Code also files as `user` entries.
fn claude_reply(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut reply: Vec<String> = Vec::new();

    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(&line)?;
        let content = entry.get("message").and_then(|message| message.get("content"));

        match entry.get("type").and_then(Value::as_str) {
            Some("user") => {
                let is_tool_result = content.and_then(Value::as_array).is_some_and(|blocks| {
                    blocks
                        .iter()
                        .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
                });
                if !is_tool_result {
                    reply.clear();
                }
            }
            Some("assistant") => match content {
                Some(Value::String(text)) => reply.push(text.clone()),
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        if block.get("type").and_then(Value::as_str) == Some("text") {
                            let text = block
                                .get("text")
                                .and_then(Value::as_str)
                                .ok_or("text must be a string")?;
                            reply.push(text.to_string());
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    Ok(reply.join("\n"))
}

/// Names the directory a checkout root's turns live under, so two
/// projects sharing one `hook-sessions` directory never collide: `root`
/// with every `/` replaced by `%`, the one character neither path ever
/// carries itself.
fn state_dir_name(root: &Path) -> String {
    root.to_string_lossy().replace('/', "%")
}

/// Names a turn's state file within its checkout's directory - stable
/// for the same client, session and turn, so two hook calls for one
/// turn share it, and different turns never collide. An empty `turn`,
/// a client that sends no `turn_id`, drops its dash rather than
/// leaving a trailing one.
fn state_file_name(client: &str, session: &str, turn: &str) -> String {
    if turn.is_empty() {
        format!("{client}-{session}")
    } else {
        format!("{client}-{session}-{turn}")
    }
}

#[cfg(test)]
mod tests;
