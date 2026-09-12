//! `percept init <client>` - writes a coding client's project config so
//! its hooks call `percept hook <client>`, and, first, the project's
//! schema files - `.percept/schemas/decisions.toml` and
//! `.percept/schemas/concepts.toml` - from the templates
//! `mapstore::templates` embeds, so a fresh checkout has maps to fold
//! before its first session opens. Run from anywhere inside a
//! checkout; the files land at the checkout root `main` resolves.
//!
//! An existing file - a schema or the client's config - is never
//! overwritten: a schema file already at that path is left exactly as
//! it is, whatever it holds, and the client's config is merged - every
//! key it already holds is kept, a hook entry naming the command to
//! write is not duplicated, and an allow string already present is not
//! repeated. Running `init` twice leaves every file byte-identical
//! after the first run.

use std::fs;
use std::path::Path;

use serde_json::{json, Map as JsonMap, Value};

use crate::cli::hook::EVENTS;
use crate::mapstore;

/// `percept init <client>` - `client` names the coding client whose
/// project config to write - `claude-code` or `codex`. `--capture`
/// adds the tool-use hook, so every tool call and its result land in
/// the log beside the prompts and replies; without it the log holds
/// what a claim can cite and none of the files the agent read.
#[derive(clap::Args)]
pub struct InitArgs {
    pub client: String,
    #[arg(long)]
    pub capture: bool,
}

/// One client's config: its file relative to the checkout root, and
/// the `Bash` patterns it may run without asking - empty for a client
/// with no such permission file.
struct Client {
    name: &'static str,
    path: &'static str,
    allow: &'static [&'static str],
}

/// The three `Bash` patterns `percept init claude-code` allows without
/// asking, so a session can read the log and its maps, and print
/// `percept start`'s render, on its own.
const CLAUDE_ALLOW: [&str; 3] = [
    "Bash(percept maps *)",
    "Bash(percept events *)",
    "Bash(percept start*)",
];

const CLIENTS: [Client; 2] = [
    Client {
        name: "claude-code",
        path: ".claude/settings.json",
        allow: &CLAUDE_ALLOW,
    },
    Client {
        name: "codex",
        path: ".codex/hooks.json",
        allow: &[],
    },
];

/// Writes the shipped schema files and `args.client`'s config under
/// `checkout`, printing one line naming what each did.
pub fn run(args: InitArgs, checkout: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let client = CLIENTS
        .iter()
        .find(|client| client.name == args.client)
        .ok_or_else(|| {
            format!(
                "{:?} names no client; percept init knows claude-code and codex",
                args.client
            )
        })?;
    for (name, text) in mapstore::templates() {
        write_schema(checkout, name, text)?;
    }
    let command = format!("percept hook {}", client.name);
    let events = hook_events(args.capture);
    write_config(checkout, client.path, |root| {
        merge(root, &command, &events, client.allow)
    })
}

/// Writes `.percept/schemas/<name>.toml` under `checkout` from `text`
/// unless a file is already there, whatever it holds - a project's own
/// schema is never overwritten. Prints `wrote <rel>` or `unchanged
/// <rel>`, the same style `write_config` uses.
fn write_schema(checkout: &Path, name: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let rel = format!(".percept/schemas/{name}.toml");
    let path = checkout.join(&rel);
    if path.exists() {
        println!("unchanged {rel}");
        return Ok(());
    }
    write_new(&path, &rel, text)
}

/// Writes `text` to `path`, creating its directory, and prints
/// `wrote <rel>` - the tail both `write_schema` and `write_config`
/// end in.
fn write_new(path: &Path, rel: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)?;
    println!("wrote {rel}");
    Ok(())
}

/// The hook events `init` writes: every one in `EVENTS` with
/// `capture`, and all but `PostToolUse` without it.
fn hook_events(capture: bool) -> Vec<&'static str> {
    EVENTS
        .iter()
        .copied()
        .filter(|event| capture || *event != "PostToolUse")
        .collect()
}

/// Reads `checkout/rel` - an empty object when it doesn't exist -
/// applies `merge`, and writes it back only when the rendered text
/// differs from what was read. Prints `wrote <rel>` or `unchanged
/// <rel>`.
fn write_config(
    checkout: &Path,
    rel: &str,
    merge: impl FnOnce(JsonMap<String, Value>) -> Result<Value, Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = checkout.join(rel);
    let original_text = fs::read_to_string(&path).unwrap_or_default();
    let updated = merge(read_or_empty(&path)?)?;
    let mut text = serde_json::to_string_pretty(&updated)?;
    text.push('\n');
    if text == original_text {
        println!("unchanged {rel}");
        return Ok(());
    }
    write_new(&path, rel, &text)
}

/// `path`'s parsed contents as an object - empty when it doesn't exist
/// or holds nothing but whitespace. Anything else that isn't a JSON
/// object is an error.
fn read_or_empty(path: &Path) -> Result<JsonMap<String, Value>, Box<dyn std::error::Error>> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(JsonMap::new()),
        Err(err) => return Err(err.into()),
    };
    if text.trim().is_empty() {
        return Ok(JsonMap::new());
    }
    match serde_json::from_str(&text)? {
        Value::Object(map) => Ok(map),
        _ => Err(format!("{} is not a JSON object", path.display()).into()),
    }
}

/// `root`'s merge: a hook entry per event in `events`, plus `allow`'s
/// patterns under `permissions.allow` when there are any.
fn merge(
    mut root: JsonMap<String, Value>,
    command: &str,
    events: &[&str],
    allow: &[&str],
) -> Result<Value, Box<dyn std::error::Error>> {
    merge_hooks(&mut root, command, events)?;
    if !allow.is_empty() {
        merge_allow(&mut root, allow)?;
    }
    Ok(Value::Object(root))
}

/// Adds `command` under `root["hooks"][event]` for every event in
/// `events`, one entry each, skipping an event that already carries a
/// hook entry naming `command`; any other entry under the same event
/// is kept, so an entry `init` no longer writes is never removed.
fn merge_hooks(
    root: &mut JsonMap<String, Value>,
    command: &str,
    events: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("hooks is not a JSON object")?;
    for event in events.iter().copied() {
        let entries = hooks
            .entry(event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event} is not an array"))?;
        if !entries.iter().any(|entry| has_command(entry, command)) {
            entries.push(hook_entry(command));
        }
    }
    Ok(())
}

/// Whether `entry` is the unscoped entry `merge_hooks` writes for
/// `command`: no `matcher`, and `command` among its nested `hooks` list.
fn has_command(entry: &Value, command: &str) -> bool {
    entry.get("matcher").is_none()
        && entry
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

/// Adds `allow`'s patterns under `root["permissions"]["allow"]`,
/// skipping a string already present and keeping every other entry.
fn merge_allow(
    root: &mut JsonMap<String, Value>,
    allow: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let permissions = root
        .entry("permissions")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("permissions is not a JSON object")?;
    let list = permissions
        .entry("allow")
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or("permissions.allow is not an array")?;
    for pattern in allow {
        if !list.iter().any(|value| value.as_str() == Some(*pattern)) {
            list.push(Value::String((*pattern).to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
