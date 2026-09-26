//! `percept init <client>` - writes a coding client's project config so
//! its hooks call `percept hook <client>`, and, first, the project's
//! schema files under `.percept/schemas/`, one per template
//! `mapstore::templates` embeds, so a fresh checkout has a map to fold
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
use crate::core::EventLog;
use crate::mapstore;

/// `percept init <client>` - `client` names the coding client whose
/// project config to write - `claude-code` or `codex`. Every hook in
/// `EVENTS` is written, the tool-use one included: a log of prompts
/// and replies alone holds nothing a map can cite but the
/// conversation, so what the agent read and ran is recorded too.
#[derive(clap::Args)]
pub struct InitArgs {
    pub client: String,
}

/// One client's config: its file relative to the checkout root, and
/// the `Bash` patterns it may run without asking - empty for a client
/// with no such permission file.
struct Client {
    name: &'static str,
    path: &'static str,
    allow: &'static [&'static str],
}

/// The `Bash` patterns `percept init claude-code` allows without
/// asking, so a session can read the log and its maps, and write to
/// them, on its own.
const CLAUDE_ALLOW: [&str; 4] = [
    "Bash(percept add *)",
    "Bash(percept remove *)",
    "Bash(percept maps *)",
    "Bash(percept events *)",
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
/// `checkout`, printing one line naming what each did. `home`, when
/// given, is where a global schema's map is minted - see
/// `mapstore::load_schemas`.
pub fn run(
    args: InitArgs,
    checkout: &Path,
    log: &dyn EventLog,
    source: &crate::core::Source,
    home: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
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
        write_schema(checkout, home, &name, text)?;
    }
    let schemas = mapstore::load_schemas(checkout, home)?;
    mapstore::ensure_maps(log, &schemas, source)?;
    let command = format!("percept hook {}", client.name);
    write_config(checkout, client.path, |root| {
        merge(root, &command, &EVENTS, client.allow)
    })
}

/// Writes `.percept/schemas/<name>.toml` under `checkout` from `text`
/// unless a file is already there, whatever it holds - a project's own
/// schema is never overwritten - or `home` declares it, since a schema
/// is global or a project's, never both. Prints `wrote <rel>`,
/// `unchanged <rel>`, or `global <rel>`, the same style
/// `write_config` uses.
fn write_schema(
    checkout: &Path,
    home: Option<&Path>,
    name: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let rel = format!("{}/{name}.toml", mapstore::SCHEMAS_DIR);
    if checkout.join(&rel).exists() {
        println!("unchanged {rel}");
        return Ok(());
    }
    if home.is_some_and(|home| home.join(&rel).exists()) {
        println!("global {rel}");
        return Ok(());
    }
    write_new(checkout, &rel, text)
}

/// Writes `text` to `checkout/rel`, creating its directory, and prints
/// `wrote <rel>` - the tail both `write_schema` and `write_config`
/// end in.
fn write_new(checkout: &Path, rel: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = checkout.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, text)?;
    println!("wrote {rel}");
    Ok(())
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
    write_new(checkout, rel, &text)
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
