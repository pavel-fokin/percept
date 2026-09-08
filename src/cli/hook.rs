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
//! A turn's events cite one another through a per-turn state file kept
//! beside the log, under `hook-sessions/<root>` - `root`'s slashes
//! replaced by `%`, so one project's turns never collide with
//! another's: the id of the turn's prompt event, so a later tool call
//! or reply can name it as its cause. The file is also the lock - held
//! exclusively for the length of one hook call - so two hook calls for
//! the same turn never race.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde_json::{json, Map as JsonMap, Value};

use crate::core::{Actor, Event, EventId, EventLog, Source};
use crate::store::{self, Lock};

/// `percept hook <client>` - `client` names the writer whose turn this
/// is, and becomes every event's source.
#[derive(clap::Args)]
pub struct HookArgs {
    /// The coding client running the hook - `claude-code`, `codex`, or
    /// any non-empty name. Becomes the event's source.
    pub client: String,
}

/// One hook JSON object's client-independent shape: `hook_event_name`,
/// `cwd`, `session_id`, and an optional `turn_id` mean the same thing
/// whichever client sent them, so `read` checks them once. Every other
/// field - `prompt`, `tool_name`, `last_assistant_message`, and so on -
/// stays in `data` for the event handlers in `run` to read.
pub struct HookInput {
    event: String,
    cwd: String,
    session: String,
    turn: String,
    data: JsonMap<String, Value>,
}

impl HookInput {
    /// The client's own working directory - what `main` resolves the
    /// checkout and project root from, since the process's own cwd may
    /// be anywhere the client's shell happened to start it.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }
}

/// Reads one hook JSON object from `input` in full and validates its
/// client-independent fields. Reading `input` to the end before
/// returning, even on a validation error, means a caller that runs
/// this before doing anything else never leaves the client's own pipe
/// half read.
pub fn read(input: &mut dyn Read) -> Result<HookInput, Box<dyn std::error::Error>> {
    let mut text = String::new();
    input.read_to_string(&mut text)?;
    let data: Value = serde_json::from_str(&text)?;
    let data = data
        .as_object()
        .ok_or("hook input must be a JSON object")?
        .clone();

    let event = text_field(&data, "hook_event_name")?;
    if !matches!(event.as_str(), "UserPromptSubmit" | "PostToolUse" | "Stop") {
        return Err(format!("unsupported hook event {event:?}").into());
    }

    let cwd = text_field(&data, "cwd")?;

    let session = text_field(&data, "session_id")?;
    if session.is_empty() {
        return Err("session_id must not be empty".into());
    }
    let turn = match data.get("turn_id") {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err("turn_id must be a string".into()),
    };

    Ok(HookInput {
        event,
        cwd,
        session,
        turn,
        data,
    })
}

/// Appends the events `input`'s event implies to `log` under `source`,
/// and returns the JSON object the client expects back on stdout -
/// `{}` unless the event asks for something. `client` names the
/// writer, checked here rather than in `read` since it comes from the
/// CLI, not the JSON; `source` is `client`'s events' project root,
/// resolved by the caller from `input.cwd`. `sessions_dir` holds one
/// directory per checkout root, created if missing.
pub fn run(
    input: HookInput,
    client: &str,
    source: &Source,
    log: &dyn EventLog,
    sessions_dir: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    if client.trim().is_empty() {
        return Err("client name must not be empty".into());
    }

    let state_path = sessions_dir
        .join(state_dir_name(&source.path))
        .join(state_file_name(client, &input.session, &input.turn));
    fs::create_dir_all(state_path.parent().expect("state path has a parent"))?;
    let state = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&state_path)?;
    // Held for the whole call, so two hook calls for the same turn
    // never race; every read and write below goes through `&File`, the
    // shared reference this lock already borrows, the same pattern
    // `store::Jsonl` uses over its own file.
    let _lock = Lock::exclusive(&state)?;

    match input.event.as_str() {
        "UserPromptSubmit" => submit_prompt(&input.data, source, log, &state, &input.event),
        "PostToolUse" => {
            let cause = read_cause(&state)?;
            record_tool_use(&input.data, source, log, cause)
        }
        "Stop" => {
            let cause = read_cause(&state)?;
            let output = record_stop(&input.data, source, log, cause);
            let _ = fs::remove_file(&state_path);
            output
        }
        // Checked in `read`.
        _ => unreachable!(),
    }
}

/// The previous cause a turn's state file holds - the prompt event's
/// id - or `None` for a turn that never recorded a prompt, or whose
/// prompt failed and cleared it.
fn read_cause(mut state: &File) -> Result<Option<EventId>, Box<dyn std::error::Error>> {
    state.seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    state.read_to_string(&mut text)?;
    let text = text.trim();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(store::parse_event_id(text)?))
    }
}

/// `UserPromptSubmit`: clears the turn's previous cause before doing
/// anything else, so a prompt that then fails to parse or commit never
/// leaves a later event citing the wrong one. Records the prompt as
/// `message.received` from `user`, stores its id as the turn's cause,
/// and returns the client's expected `additionalContext`.
fn submit_prompt(
    data: &JsonMap<String, Value>,
    source: &Source,
    log: &dyn EventLog,
    mut state: &File,
    event: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    state.set_len(0)?;
    state.seek(SeekFrom::Start(0))?;

    let prompt = text_field(data, "prompt")?;
    let committed = Event::message_received(Actor::User, prompt, source.clone(), None);
    let id = committed.id();
    log.append(&committed)?;
    state.write_all(id.as_uuid().to_string().as_bytes())?;

    Ok(json!({
        "hookSpecificOutput": {
            "hookEventName": event,
            "additionalContext": format!("percept event {}", id.as_uuid()),
        }
    }))
}

/// `PostToolUse`: records the call, caused by the turn's prompt, then
/// its result, caused by the call.
fn record_tool_use(
    data: &JsonMap<String, Value>,
    source: &Source,
    log: &dyn EventLog,
    cause: Option<EventId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let tool = text_field(data, "tool_name")?;
    let arguments = data.get("tool_input").cloned().ok_or("missing tool_input")?;
    let arguments = serde_json::to_string(&arguments)?;
    let response = data
        .get("tool_response")
        .cloned()
        .ok_or("missing tool_response")?;
    let response = match response {
        Value::String(text) => text,
        other => serde_json::to_string(&other)?,
    };

    let call = Event::tool_called(tool, arguments, source.clone(), cause);
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
    data: &JsonMap<String, Value>,
    source: &Source,
    log: &dyn EventLog,
    cause: Option<EventId>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let reply = match data.get("last_assistant_message") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Null) | None => match data.get("transcript_path") {
            Some(Value::Null) | None => None,
            Some(Value::String(text)) if text.is_empty() => None,
            Some(Value::String(path)) => Some(claude_reply(Path::new(path))?),
            Some(_) => return Err("transcript_path must be a string".into()),
        },
        Some(_) => return Err("last_assistant_message must be a string".into()),
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
        let content = entry
            .get("message")
            .and_then(|message| message.get("content"))
            .cloned()
            .unwrap_or(Value::Array(Vec::new()));

        match entry.get("type").and_then(Value::as_str) {
            Some("user") => {
                let is_tool_result = content.as_array().is_some_and(|blocks| {
                    blocks
                        .iter()
                        .any(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
                });
                if content.is_string() || !is_tool_result {
                    reply.clear();
                }
            }
            Some("assistant") => {
                if let Some(text) = content.as_str() {
                    reply.push(text.to_string());
                } else if let Some(blocks) = content.as_array() {
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
            }
            _ => {}
        }
    }

    Ok(reply.join("\n"))
}

/// Reads `data[name]` as a string, erroring when it's missing or holds
/// another type.
fn text_field(
    data: &JsonMap<String, Value>,
    name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    match data.get(name) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(format!("{name} must be a string").into()),
        None => Err(format!("missing {name}").into()),
    }
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
