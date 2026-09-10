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

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::{cited_label, Actor, Event, EventId, EventLog, Map, Node, Payload, Schemas, Source};
use crate::mapstore::{block_header, capped_lines, judged_since_block, latest_session_per_client, line_id};
use crate::shared::Timestamp;
use crate::store::TurnState;
use crate::workspace;

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
    checkout: &Path,
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
        if let Some(block) = judged_since_block(&maps, at) {
            sections.push(block);
        }
    }
    if let Some(block) = changed_since_recorded_block(&maps, &events, checkout) {
        sections.push(block);
    }
    let (open_blocks, pointer) = open_blocks_and_pointer(&maps);
    sections.extend(open_blocks);
    sections.extend(pointer);
    sections.push(RULES.to_string());

    Ok(json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": sections.join("\n\n"),
        }
    }))
}

/// The recording rules, printed after the fragment on every session
/// start. A stranger's project has no AGENTS.md or skill naming them,
/// so this block is the one place the model meets them, and the
/// recipe is complete enough to run as printed.
const RULES: &str = "\
recording
- Before proposing a design, look: percept maps show decisions --around <id>, or --format md for the whole map.
- When the user says yes to a proposal, record it at once, citing the prompt id the hook printed:
    percept maps record decisions --actor agent --source <prompt id> <<'EOF'
    question \"what was asked\"
    decision \"what was chosen\"
      why \"the grounds\"
      resolves question
    option \"an alternative that lost\"
      why \"why it lost\"
      answers question
    EOF
- A claim that rests on a file cites the text it read: an indented line, cites src/path.rs:10-20, under the node.
- A decision that changes an earlier one adds a supersedes <id> line under it; never remove a node.
- A decision that no longer seems to fit is not yours to rewrite: raise a question with a reopens <id> line under it, and let the user settle it.
- A node marked disputed carries the human's why: never propose it again; a correction the user agrees is a new decision with a supersedes line.
- Close a task by changing it, not by adding a node: t4 on its own line, then state \"done\" and outcome \"<commit>: what happened\" indented under it (state \"dropped\" and why for one dropped, state \"open\" to reopen one). A task the user wrote takes only state and outcome from you; its name and why are theirs.
- Close the session with one line naming what was recorded: Recorded to decisions: q1, d1, o1.";

/// What each folded map gained since `since`: a counts line for every
/// map, in fold order, then up to `mapstore::judge::LIMIT` lines per
/// map that gained anything - a node's `added_at` is compared
/// directly, not `Map::since`, which would also surface an older node
/// a fresh edge only touched.
fn gained_block(maps: &[Map], since: Timestamp) -> String {
    let per_map: Vec<Vec<&Node>> = maps
        .iter()
        .map(|map| map.headlines().filter(|node| node.changed_at >= since).collect())
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
        lines.extend(capped_lines(
            gained
                .iter()
                .map(|node| format!("{} {} {:?}", line_id(map, node), node.kind, node.name))
                .collect(),
        ));
    }
    lines.join("\n")
}

/// What every current headline node cites that no longer matches the
/// working tree - a citation whose file moved on since it was
/// seen. `None` when nothing changed, so `start_session` omits
/// the block entirely rather than printing an empty one.
///
/// Builds two indexes over `events` once, both over `file.cited`
/// events only - id to event, and causation id to the events it
/// caused - so no node's check re-reads the log: `id_to_event`
/// resolves a node's `sources` entries, `later_citations` walks a
/// citation forward to the newest re-citation of the same file before
/// it is checked against the tree. `cache` memoises each cited path's
/// normalised tree text - `None` for one that is gone - for the rest
/// of this call, so a path cited by more than one node is read once.
fn changed_since_recorded_block(maps: &[Map], events: &[Event], checkout: &Path) -> Option<String> {
    let file_cited: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event.payload(), Payload::FileCited { .. }))
        .collect();
    let id_to_event: HashMap<EventId, &Event> =
        file_cited.iter().map(|event| (event.id(), *event)).collect();
    let mut later_citations: HashMap<EventId, Vec<&Event>> = HashMap::new();
    for event in &file_cited {
        if let Some(cause) = event.causation_id() {
            later_citations.entry(cause).or_default().push(event);
        }
    }

    let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut lines: Vec<String> = Vec::new();
    for map in maps {
        for node in map.headlines() {
            let findings = node_changes(node, &id_to_event, &later_citations, checkout, &mut cache);
            if !findings.is_empty() {
                lines.push(format!("{} cites {}", line_id(map, node), findings.join(", ")));
            }
        }
    }

    if lines.is_empty() {
        return None;
    }

    let mut block = vec![block_header("changed since recorded", lines.len())];
    block.extend(capped_lines(lines));
    Some(block.join("\n"))
}

/// `node`'s own `changed`/`gone` findings, one per source that names a
/// `file.cited` event, checked at its newest re-citation.
fn node_changes(
    node: &Node,
    id_to_event: &HashMap<EventId, &Event>,
    later_citations: &HashMap<EventId, Vec<&Event>>,
    checkout: &Path,
    cache: &mut HashMap<PathBuf, Option<String>>,
) -> Vec<String> {
    node.sources
        .iter()
        .filter_map(|source_id| {
            let event = id_to_event.get(source_id)?;
            let newest = newest_citation(event, later_citations);
            let Payload::FileCited { path, lines, excerpt } = newest.payload() else {
                return None;
            };
            citation_status(cache, checkout, path, excerpt)
                .map(|status| format!("{} {status}", cited_label(path, *lines)))
        })
        .collect()
}

/// Follows `event` forward through `later_citations`, each hop the
/// latest re-citation of the same file caused by the one before it -
/// a later citation of a different path is not a re-citation of this
/// one, so it is ignored. A visited set stops a causation cycle a
/// hand-edited log could hold from spinning forever.
fn newest_citation<'a>(
    event: &'a Event,
    later_citations: &HashMap<EventId, Vec<&'a Event>>,
) -> &'a Event {
    let Payload::FileCited { path, .. } = event.payload() else {
        return event;
    };
    let mut current = event;
    let mut visited = HashSet::from([event.id()]);
    while let Some(next) = later_citations
        .get(&current.id())
        .into_iter()
        .flatten()
        .filter(|candidate| {
            matches!(candidate.payload(), Payload::FileCited { path: p, .. } if p == path)
        })
        .filter(|candidate| visited.insert(candidate.id()))
        .max_by_key(|candidate| candidate.created_at())
    {
        current = next;
    }
    current
}

/// `gone` when `path` under `checkout` is missing or binary; `changed`
/// when it no longer contains `excerpt` as a substring, or `excerpt`
/// normalises to nothing to compare against; `None` when it still
/// reads. Both sides go through `normalize`; the tree's side is read
/// through `cache`, so a path more than one citation names is read and
/// normalised once.
fn citation_status(
    cache: &mut HashMap<PathBuf, Option<String>>,
    checkout: &Path,
    path: &Path,
    excerpt: &str,
) -> Option<&'static str> {
    let excerpt = normalize(excerpt);
    if excerpt.is_empty() {
        return Some("changed");
    }
    let text = cache
        .entry(path.to_path_buf())
        .or_insert_with(|| workspace::read_text_lossy(&checkout.join(path)).ok().map(|t| normalize(&t)));
    match text {
        Some(text) if text.contains(&excerpt) => None,
        _ => Some(if text.is_some() { "changed" } else { "gone" }),
    }
}

/// Each line's trailing whitespace stripped, then leading and trailing
/// blank lines trimmed - the one normalisation both sides of a
/// `changed since recorded` comparison go through, so a citation whose
/// stored excerpt padded its range with context still matches.
fn normalize(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|line| line.trim_end()).collect();
    let start = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(start, |i| i + 1);
    lines[start..end].join("\n")
}

/// One `open {kind} (...)` block per map that has open items - a map
/// with neither a `Settlement` nor a headline kind that declares
/// states (an `ideas` map, say) is skipped entirely, never by name,
/// since `Map::open` is empty there - plus the fragment pointer at the
/// first open item found, walking maps in fold order.
fn open_blocks_and_pointer(maps: &[Map]) -> (Vec<String>, Option<String>) {
    let mut blocks = Vec::new();
    let mut pointer = None;

    for map in maps {
        let open: Vec<&Node> = map.open().collect();
        let Some(first) = open.first() else {
            continue;
        };

        let mut lines = vec![block_header(&format!("open {}", first.kind), open.len())];
        lines.extend(capped_lines(
            open.iter()
                .map(|node| {
                    let mut line = format!("{} {:?}", line_id(map, node), node.name);
                    for decision in map.reopens(node.id) {
                        line.push_str(&format!(" reopens {}", line_id(map, decision)));
                    }
                    line
                })
                .collect(),
        ));
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
    latest_session_per_client(events, &source.scope())
        .get(&(source.name.clone(), source.path.clone()))
        .copied()
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
