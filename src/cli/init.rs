//! `percept init <client>` - writes a coding client's project config so
//! its hooks call `percept hook <client>`, the in-process replacement
//! for `scripts/agent-hook.py`. Run from anywhere inside a checkout;
//! the files land at the checkout root `main` resolves.
//!
//! An existing file is merged, never overwritten: every key it already
//! holds is kept, a hook entry naming the command to write is not
//! duplicated, and an allow string already present is not repeated.
//! Running `init` twice leaves the file byte-identical after the
//! first run.

use std::fs;
use std::path::Path;

use serde_json::{json, Map as JsonMap, Value};

/// `percept init <client>` - `client` names the coding client whose
/// project config to write: `claude-code` or `codex`.
#[derive(clap::Args)]
pub struct InitArgs {
    /// The coding client to write config for - `claude-code` or `codex`.
    pub client: String,
}

/// The hook events every client's config wires to `percept hook`.
const EVENTS: [&str; 3] = ["UserPromptSubmit", "PostToolUse", "Stop"];

/// The two `Bash` patterns `percept init claude-code` allows without
/// asking, so a session can read the log and its maps on its own.
const CLAUDE_ALLOW: [&str; 2] = ["Bash(percept maps *)", "Bash(percept events *)"];

/// Writes `client`'s config under `checkout`, printing one line per
/// file naming what it did.
pub fn run(args: InitArgs, checkout: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match args.client.as_str() {
        "claude-code" => write_config(checkout, ".claude/settings.json", |root| {
            merge_claude_code(root, "percept hook claude-code")
        }),
        "codex" => write_config(checkout, ".codex/hooks.json", |root| {
            merge_codex(root, "percept hook codex")
        }),
        other => Err(format!(
            "{other:?} names no client; percept init knows claude-code and codex"
        )
        .into()),
    }
}

/// Reads `checkout/rel` - an empty object when it doesn't exist -
/// applies `merge`, and writes it back only when it changed. Prints
/// `wrote <rel>` or `unchanged <rel>`.
fn write_config(
    checkout: &Path,
    rel: &str,
    merge: impl FnOnce(Value) -> Result<Value, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = checkout.join(rel);
    let original = read_or_empty(&path)?;
    if !original.is_object() {
        return Err(format!("{rel} is not a JSON object").into());
    }
    let updated = merge(original.clone())?;
    if updated == original {
        println!("unchanged {rel}");
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(&updated)?;
    text.push('\n');
    fs::write(&path, text)?;
    println!("wrote {rel}");
    Ok(())
}

/// `path`'s parsed contents, or an empty object when it doesn't exist.
fn read_or_empty(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(serde_json::from_str(&text)?),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(err) => Err(err.into()),
    }
}

/// `.claude/settings.json`'s merge: the three hook events, plus the
/// two `Bash` patterns under `permissions.allow`.
fn merge_claude_code(root: Value, command: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut root = as_object(root, "settings")?;
    merge_hooks(&mut root, command)?;
    merge_allow(&mut root)?;
    Ok(Value::Object(root))
}

/// `.codex/hooks.json`'s merge: the three hook events, no permissions.
fn merge_codex(root: Value, command: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut root = as_object(root, "hooks file")?;
    merge_hooks(&mut root, command)?;
    Ok(Value::Object(root))
}

fn as_object(
    value: Value,
    name: &str,
) -> Result<JsonMap<String, Value>, Box<dyn std::error::Error>> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{name} is not a JSON object").into()),
    }
}

/// Adds `command` under `root["hooks"][event]` for every event in
/// `EVENTS`, one entry each, skipping an event that already carries a
/// hook entry naming `command`; any other entry under the same event
/// is kept.
fn merge_hooks(
    root: &mut JsonMap<String, Value>,
    command: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("hooks is not a JSON object")?;
    for event in EVENTS {
        let entries = hooks
            .entry(event.to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} is not an array"))?;
        if !entries.iter().any(|entry| has_command(entry, command)) {
            entries.push(hook_entry(command));
        }
    }
    Ok(())
}

/// Whether `entry` - one item of a hook event's array - already runs
/// `command`, looked up in its nested `hooks` list, the shape every
/// entry here carries.
fn has_command(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks
                .iter()
                .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
        })
}

/// One hook event's entry: a single command, run with a 20 second
/// timeout, the shape this repo's own client configs already use.
fn hook_entry(command: &str) -> Value {
    json!({
        "hooks": [
            { "type": "command", "command": command, "timeout": 20 }
        ]
    })
}

/// Adds `CLAUDE_ALLOW` under `root["permissions"]["allow"]`, skipping a
/// string already present and keeping every other entry.
fn merge_allow(root: &mut JsonMap<String, Value>) -> Result<(), Box<dyn std::error::Error>> {
    let permissions = root
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("permissions is not a JSON object")?;
    let allow = permissions
        .entry("allow".to_string())
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("permissions.allow is not an array")?;
    for pattern in CLAUDE_ALLOW {
        if !allow.iter().any(|value| value.as_str() == Some(pattern)) {
            allow.push(Value::String(pattern.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
