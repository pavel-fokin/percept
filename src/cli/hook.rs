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
//!
//! `SessionStart`'s `additionalContext` is exactly `start::render`'s
//! output - the same text `percept start` prints from the shell.

use std::fs::File;
use std::io::{BufRead, Read};
use std::path::Path;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{Actor, Event, EventId, EventLog, Schemas, Source};
use crate::mapstore::of_path;
use crate::store::TurnState;

use super::start;

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
    me: Option<crate::core::HumanId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let dir = sessions_dir.join(state_dir_name(&source.path));
    let name = state_file_name(&source.name, &input.session_id, &input.turn_id);
    let mut state = TurnState::open(&dir, &name)?;

    match input.event {
        HookEvent::SessionStart {} => {
            let schemas = crate::mapstore::load_schemas(checkout)?;
            start_session(source, log, &schemas, checkout)
        }
        HookEvent::UserPromptSubmit { prompt } => submit_prompt(prompt, source, log, &mut state, me),
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

/// `SessionStart`: folds every log-backed schema from the events
/// recorded before this call, so `start::render`'s own since-cut finds
/// the previous session and not this one, then records a fresh
/// `session.started` for the next call to find. The
/// `additionalContext` is exactly what `percept start` prints from the
/// shell.
fn start_session(
    source: &Source,
    log: &dyn EventLog,
    schemas: &Schemas,
    checkout: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    let events = log.load()?;
    let maps = schemas.fold_all(of_path(&events, &source.path))?;
    let rendered = start::render(&maps, &events, &source.path, checkout);

    log.append(&Event::session_started(source.clone()))?;

    Ok(json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": rendered,
        }
    }))
}

/// `UserPromptSubmit`: clears the turn's previous cause before doing
/// anything else, so a prompt that then fails to commit never leaves a
/// later event citing the wrong one. Records the prompt as
/// `message.received` from `human`, stores its id as the turn's cause,
/// and returns the client's expected `additionalContext`.
fn submit_prompt(
    prompt: String,
    source: &Source,
    log: &dyn EventLog,
    state: &mut TurnState,
    me: Option<crate::core::HumanId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    state.clear()?;

    let committed = Event::message_received(Actor::Human(me), prompt, source.clone(), None);
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
/// reply is recorded as `message.received` from `agent`, caused by the
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
            let committed = Event::message_received(Actor::Agent, reply, source.clone(), cause);
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
