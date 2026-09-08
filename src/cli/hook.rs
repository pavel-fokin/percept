//! `percept hook <client>` - what `scripts/agent-hook.py` used to run
//! as a subprocess, now in-process. Reads one hook JSON object off
//! stdin, the shape Claude Code and Codex both send, and turns it into
//! events on the same log every other subcommand appends to. Never
//! fails the client's turn: `main` catches every error here, prints it
//! to stderr as `percept hook: <error>`, and still prints `{}` to
//! stdout.
//!
//! A turn's events cite one another through a per-turn state file kept
//! beside the log, under `hook-sessions`: the id of the turn's prompt
//! event, so a later tool call or reply can name it as its cause. The
//! file is also the lock - held exclusively for the length of one hook
//! call - so two hook calls for the same turn never race.

use std::collections::hash_map::DefaultHasher;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde_json::{json, Map as JsonMap, Value};

use crate::core::{Actor, EventId, EventLog, Source};
use crate::store;

/// `percept hook <client>` - `client` names the writer whose turn this
/// is, and becomes every event's source.
#[derive(clap::Args)]
pub struct HookArgs {
    /// The coding client running the hook - `claude-code`, `codex`, or
    /// any non-empty name. Becomes the event's source.
    pub client: String,
}

/// Reads one hook JSON object from `input`, appends the events it
/// implies to `log` under `client`'s source, and returns the JSON
/// object the client expects back on stdout - `{}` unless the event
/// asks for something. `sessions_dir` holds one file per turn, created
/// if missing.
pub fn run(
    client: &str,
    input: &mut dyn Read,
    log: &dyn EventLog,
    sessions_dir: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    if client.trim().is_empty() {
        return Err("client name must not be empty".into());
    }

    let mut text = String::new();
    input.read_to_string(&mut text)?;
    let data: Value = serde_json::from_str(&text)?;
    let data = data
        .as_object()
        .ok_or("hook input must be a JSON object")?;

    let event = text_field(data, "hook_event_name")?;
    if !matches!(event.as_str(), "UserPromptSubmit" | "PostToolUse" | "Stop") {
        return Err(format!("unsupported hook event {event:?}").into());
    }

    let cwd = text_field(data, "cwd")?;
    let checkout = crate::root_for(Path::new(&cwd))?;
    let root = crate::project_of(&checkout);

    let session = text_field(data, "session_id")?;
    if session.is_empty() {
        return Err("session_id must not be empty".into());
    }
    let turn = match data.get("turn_id") {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => return Err("turn_id must be a string".into()),
    };

    fs::create_dir_all(sessions_dir)?;
    let state_path = sessions_dir.join(state_key(client, &root, &session, &turn));
    let mut state = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&state_path)?;
    state.lock()?;

    let source = Source {
        name: client.to_string(),
        path: root,
    };

    match event.as_str() {
        "UserPromptSubmit" => submit_prompt(data, &source, log, &mut state, &event),
        "PostToolUse" => {
            let cause = read_cause(&mut state)?;
            record_tool_use(data, &source, log, cause)
        }
        "Stop" => {
            let cause = read_cause(&mut state)?;
            let output = record_stop(data, &source, log, cause);
            drop(state);
            let _ = fs::remove_file(&state_path);
            output
        }
        // Checked above.
        _ => unreachable!(),
    }
}

/// The previous cause a turn's state file holds - the prompt event's
/// id - or `None` for a turn that never recorded a prompt, or whose
/// prompt failed and cleared it.
fn read_cause(state: &mut File) -> Result<Option<EventId>, Box<dyn std::error::Error>> {
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
    state: &mut File,
    event: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    state.set_len(0)?;
    state.seek(SeekFrom::Start(0))?;

    let prompt = text_field(data, "prompt")?;
    let id = append(
        log,
        source,
        Actor::User,
        "message.received",
        json!({ "content": prompt }),
        None,
    )?;
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
    let response = data
        .get("tool_response")
        .cloned()
        .ok_or("missing tool_response")?;
    let response = match response {
        Value::String(text) => text,
        other => serde_json::to_string(&other)?,
    };

    let call = append(
        log,
        source,
        Actor::Model,
        "tool.called",
        json!({ "tool": tool, "arguments": arguments }),
        cause,
    )?;
    append(
        log,
        source,
        Actor::System,
        "tool.resulted",
        json!({ "content": response }),
        Some(call),
    )?;
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
            append(
                log,
                source,
                Actor::Model,
                "message.received",
                json!({ "content": reply }),
                cause,
            )?;
        }
    }
    Ok(json!({}))
}

/// Builds one event through `store::decode` - the same path
/// `cli::publish` uses - and appends it to `log`, returning its id.
fn append(
    log: &dyn EventLog,
    source: &Source,
    actor: Actor,
    kind: &str,
    payload: Value,
    causation_id: Option<EventId>,
) -> Result<EventId, Box<dyn std::error::Error>> {
    let event = store::decode(actor.name(), source.clone(), kind, causation_id, payload)?;
    let id = event.id();
    log.append(&event)?;
    Ok(id)
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

/// Names a turn's state file - stable for the same client, root,
/// session and turn, so two hook calls for one turn share it, and
/// different turns never collide. Not cryptographic: the file is a
/// local handle, never compared across runs of a different build.
fn state_key(client: &str, root: &Path, session: &str, turn: &str) -> String {
    let mut hasher = DefaultHasher::new();
    client.hash(&mut hasher);
    root.hash(&mut hasher);
    session.hash(&mut hasher);
    turn.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests;
