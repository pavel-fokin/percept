use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::core::testing::{content, human, map_id, FakeLog};
use crate::core::{HumanId, Payload};

/// The one map a hook test's project declares: `debates`, with a
/// `topic` node kind and nothing else - a mini schema of this
/// fixture's own, so no test here rests on a shipped template.
const DEBATES_TOML: &str = "purpose = \"what a hook test needs\"\n\
                            [nodes.topic]\n";

fn write_debates_schema(root: &Path) {
    let dir = root.join(".percept/schemas");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("debates.toml"), DEBATES_TOML).unwrap();
}

/// A checkout `run` can discover a root in - a `.percept` marker is
/// enough, so a test needs no `git init` - plus the sessions directory
/// and log `run` is given. A `SessionStart` loads schemas from this
/// same root, exactly as production does, so this fixture writes its
/// own `debates` schema there; `with_extra_schema` adds another beside
/// it.
struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    sessions: PathBuf,
    log: FakeLog,
    me: Option<HumanId>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout with spaces");
        std::fs::create_dir_all(root.join(".percept")).unwrap();
        // `run` canonicalizes the `cwd` it discovers a root from, and on
        // macOS `/var` is itself a symlink to `/private/var` - a test
        // that compares an event's source path must compare the same
        // canonical form.
        let root = root.canonicalize().unwrap();
        write_debates_schema(&root);
        let created = Event::map_created(
            map_id("debates"),
            "debates".to_string(),
            Source { name: "percept".to_string(), path: root.clone() },
        );
        Self {
            sessions: temp.path().join("storage/hook-sessions"),
            root,
            log: FakeLog::seeded(vec![created]),
            me: human(),
            _temp: temp,
        }
    }

    /// Writes `<root>/.percept/schemas/<name>.toml` beside `debates`.
    fn with_extra_schema(self, name: &str, toml: &str) -> Self {
        let dir = self.root.join(".percept/schemas");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.toml")), toml).unwrap();
        self
    }

    /// Another project, so a test can tell one checkout's cause from
    /// another's - with the same `debates` schema `new` gives this
    /// fixture's own root.
    fn other_root(&self) -> PathBuf {
        let other = self._temp.path().join("second checkout");
        std::fs::create_dir_all(other.join(".percept")).unwrap();
        let other = other.canonicalize().unwrap();
        write_debates_schema(&other);
        self.log
            .append(&Event::map_created(
                map_id("debates"),
                "debates".to_string(),
                Source { name: "percept".to_string(), path: other.clone() },
            ))
            .unwrap();
        other
    }

    fn call(&self, client: &str, body: Value) -> Result<Value, Box<dyn std::error::Error>> {
        self.call_raw(client, &body.to_string())
    }

    /// Mirrors what `main` does before dispatching to `run`: read the
    /// input, then resolve the checkout and project root from its own
    /// `cwd` rather than the process's.
    fn call_raw(&self, client: &str, raw: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let mut cursor = Cursor::new(raw.as_bytes().to_vec());
        let input = read(&mut cursor)?;
        let checkout = crate::root_for(Path::new(input.cwd()))?;
        let source = Source {
            name: client.to_string(),
            path: crate::project_of(&checkout),
        };
        run(input, &source, &self.log, &self.sessions, &checkout, self.me)
    }

    /// The number of turn state files kept anywhere under
    /// `hook-sessions`, regardless of which checkout's directory holds
    /// them.
    fn state_file_count(&self) -> usize {
        fn count(dir: &Path) -> usize {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return 0;
            };
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        count(&path)
                    } else {
                        1
                    }
                })
                .sum()
        }
        count(&self.sessions)
    }

    /// Fires `SessionStart` for `client` against this fixture's own
    /// root, returning what the client is answered.
    fn session_start(&self, client: &str) -> Value {
        self.call(
            client,
            json!({
                "hook_event_name": "SessionStart",
                "cwd": self.root.to_str().unwrap(),
                "session_id": "session",
            }),
        )
        .unwrap()
    }

    /// `prompt_at`, against this fixture's own root.
    fn prompt(&self, client: &str, session: &str, turn: &str, text: &str) -> String {
        self.prompt_at(client, &self.root.clone(), session, turn, text)
    }

    fn prompt_at(
        &self,
        client: &str,
        root: &Path,
        session: &str,
        turn: &str,
        text: &str,
    ) -> String {
        let output = self
            .call(
                client,
                json!({
                    "hook_event_name": "UserPromptSubmit",
                    "cwd": root.to_str().unwrap(),
                    "session_id": session,
                    "turn_id": turn,
                    "prompt": text,
                }),
            )
            .unwrap();
        output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .strip_prefix("percept event ")
            .unwrap()
            .to_string()
    }

    /// `stop_at`, against this fixture's own root.
    fn stop(
        &self,
        client: &str,
        session: &str,
        turn: &str,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        self.stop_at(client, &self.root.clone(), session, turn, fields)
    }

    fn stop_at(
        &self,
        client: &str,
        root: &Path,
        session: &str,
        turn: &str,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let mut body = json!({
            "hook_event_name": "Stop",
            "cwd": root.to_str().unwrap(),
            "session_id": session,
            "turn_id": turn,
        });
        merge(&mut body, fields);
        self.call(client, body)
    }

    fn subagent_stop(
        &self,
        client: &str,
        session: &str,
        turn: &str,
        fields: Value,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let mut body = json!({
            "hook_event_name": "SubagentStop",
            "cwd": self.root.to_str().unwrap(),
            "session_id": session,
            "turn_id": turn,
        });
        merge(&mut body, fields);
        self.call(client, body)
    }

    fn events(&self) -> Vec<crate::core::Event> {
        self.log
            .load()
            .unwrap()
            .into_iter()
            .filter(|event| !matches!(event.payload(), Payload::MapCreated { .. }))
            .collect()
    }

}

fn merge(base: &mut Value, extra: Value) {
    base.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
}

#[test]
fn every_hook_event_names_itself_after_deserializing() {
    let cases = [
        ("SessionStart", json!({"hook_event_name": "SessionStart"})),
        (
            "UserPromptSubmit",
            json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi"}),
        ),
        (
            "PostToolUse",
            json!({
                "hook_event_name": "PostToolUse",
                "tool_name": "Bash",
                "tool_input": {},
                "tool_response": {},
            }),
        ),
        (
            "SubagentStop",
            json!({
                "hook_event_name": "SubagentStop",
                "last_assistant_message": null,
                "agent_transcript_path": null,
            }),
        ),
        (
            "Stop",
            json!({
                "hook_event_name": "Stop",
                "last_assistant_message": null,
                "transcript_path": null,
            }),
        ),
    ];

    assert_eq!(EVENTS.len(), cases.len());
    for (name, body) in cases {
        assert!(EVENTS.contains(&name));
        let event: HookEvent = serde_json::from_value(body).unwrap();
        assert_eq!(event.name(), name);
    }
}

#[test]
fn a_broken_project_schema_does_not_stop_the_hook() {
    let fixture = Fixture::new().with_extra_schema("broken", "not valid toml");

    let id = fixture.prompt("codex", "session", "", "hello");

    assert_eq!(fixture.events().len(), 1);
    assert_eq!(fixture.events()[0].id().as_uuid().to_string(), id);
}

#[test]
fn prompt_context_names_the_committed_event() {
    let fixture = Fixture::new();
    let id = fixture.prompt("codex", "session", "", "hello");

    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id().as_uuid().to_string(), id);
    assert_eq!(content(&events[0]), "hello");
    assert!(matches!(events[0].actor(), Actor::Human(_)));
}

#[test]
fn a_subagent_hand_back_delivered_as_a_prompt_is_the_agents() {
    let fixture = Fixture::new();
    fixture.prompt(
        "claude-code",
        "session",
        "",
        "<agent-message from=\"a58cf0ad\">\n  the report\n</agent-message>",
    );

    assert_eq!(fixture.events()[0].actor(), Actor::Agent);
}

#[test]
fn a_task_notification_delivered_as_a_prompt_is_the_systems() {
    let fixture = Fixture::new();
    fixture.prompt(
        "claude-code",
        "session",
        "",
        "<task-notification>\n<status>completed</status>\n</task-notification>",
    );

    assert_eq!(fixture.events()[0].actor(), Actor::System);
}

#[test]
fn a_prompt_quoting_a_frame_is_still_the_humans() {
    let fixture = Fixture::new();
    fixture.prompt(
        "claude-code",
        "session",
        "",
        "why did <task-notification>\n</task-notification> say that?",
    );

    assert!(matches!(fixture.events()[0].actor(), Actor::Human(_)));
}

#[test]
fn a_prompt_opening_a_frame_it_never_closes_is_still_the_humans() {
    let fixture = Fixture::new();
    fixture.prompt("claude-code", "session", "", "<agent-message from=\"x\"> and then I typed on");

    assert!(matches!(fixture.events()[0].actor(), Actor::Human(_)));
}

#[test]
fn a_prompt_whose_first_word_only_starts_like_a_frame_is_the_humans() {
    let fixture = Fixture::new();
    fixture.prompt("claude-code", "session", "", "<agent-messages-are-odd>\n</agent-message>");

    assert!(matches!(fixture.events()[0].actor(), Actor::Human(_)));
}

#[test]
fn a_session_start_records_one_marker_from_the_system() {
    let fixture = Fixture::new();
    fixture.session_start("codex");

    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].payload(), Payload::SessionStarted));
    assert_eq!(events[0].actor(), Actor::System);
}

#[test]
fn a_session_start_answers_the_client_the_start_screen() {
    let fixture = Fixture::new();

    let output = fixture.session_start("codex");

    let output = &output["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "SessionStart");
    let context = output["additionalContext"].as_str().unwrap();
    assert!(context.contains("percept maps record <map>"), "{context}");
    assert!(context.contains("\n# debates\n\nwhat a hook test needs\n"), "{context}");
}

#[test]
fn a_session_start_with_a_broken_schema_still_records_its_marker() {
    let fixture = Fixture::new().with_extra_schema("broken", "not valid toml");

    let result = fixture.call(
        "codex",
        json!({
            "hook_event_name": "SessionStart",
            "cwd": fixture.root.to_str().unwrap(),
            "session_id": "session",
        }),
    );

    assert!(result.is_err());
    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].payload(), Payload::SessionStarted));
}

#[test]
fn a_returning_session_records_its_own_marker() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    fixture.session_start("codex");

    assert_eq!(fixture.events().len(), 2);
}

#[test]
fn a_prompts_context_is_the_event_id_alone() {
    let fixture = Fixture::new();

    let output = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": fixture.root.to_str().unwrap(),
                "session_id": "session",
                "prompt": "hello",
            }),
        )
        .unwrap();

    let context = output["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    let prompt = fixture.events().pop().unwrap();
    assert_eq!(context, format!("percept event {}", prompt.id().as_uuid()));
}

#[test]
fn each_client_records_its_own_session_marker() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    fixture.session_start("claude-code");

    let events = fixture.events();
    let names: Vec<&str> = events.iter().map(|event| event.source().name.as_str()).collect();
    assert_eq!(names, ["codex", "claude-code"]);
}

#[test]
fn any_client_name_becomes_the_events_source() {
    let fixture = Fixture::new();
    fixture.prompt("opencode", "session", "", "hello");

    assert_eq!(fixture.events()[0].source().name, "opencode");
}

#[test]
fn tool_result_cites_the_call_and_the_call_cites_the_prompt() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "", "hello");

    fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "PostToolUse",
                "cwd": fixture.root.to_str().unwrap(),
                "session_id": "session",
                "tool_name": "Bash",
                "tool_input": {"command": "ls"},
                "tool_response": {"stdout": "files"},
            }),
        )
        .unwrap();

    let events = fixture.events();
    let call = events
        .iter()
        .find(|event| matches!(event.payload(), Payload::ToolCalled { .. }))
        .unwrap();
    let result = events
        .iter()
        .find(|event| matches!(event.payload(), Payload::ToolResulted { .. }))
        .unwrap();

    assert_eq!(call.causation_id().unwrap().as_uuid().to_string(), prompt);
    assert_eq!(result.causation_id(), Some(call.id()));
    let Payload::ToolResulted { content } = result.payload() else {
        panic!("expected a tool.resulted event")
    };
    let stored: Value = serde_json::from_str(content).unwrap();
    assert_eq!(stored, json!({"stdout": "files"}));
}

#[test]
fn stop_uses_last_assistant_message_and_the_prompts_cause() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "", "hello");

    let output = fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    assert_eq!(output, json!({}));
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
    assert_eq!(content(&reply), "reply");
}

#[test]
fn subagent_stop_records_an_agent_reply_caused_by_the_parent_prompt() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "turn", "hello");

    let output = fixture
        .subagent_stop(
            "codex",
            "session",
            "turn",
            json!({"last_assistant_message": "subagent reply"}),
        )
        .unwrap();

    assert_eq!(output, json!({}));
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
    assert_eq!(content(&reply), "subagent reply");
}

#[test]
fn subagent_stop_keeps_the_parent_turns_state() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "turn", "hello");

    fixture
        .subagent_stop(
            "codex",
            "session",
            "turn",
            json!({"last_assistant_message": "subagent reply"}),
        )
        .unwrap();

    let dir = turn_dir(&fixture.sessions, &fixture.root);
    assert_eq!(TurnState::latest_cause(&dir).unwrap().unwrap().as_uuid().to_string(), prompt);
}

#[test]
fn subagent_stop_falls_back_to_the_agent_transcript() {
    let fixture = Fixture::new();
    let transcript = fixture._temp.path().join("subagent transcript.jsonl");
    let entries = [
        json!({"type": "user", "message": {"content": "task"}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "subagent reply"}]}}),
    ];
    let text = entries
        .iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&transcript, text).unwrap();

    fixture
        .subagent_stop(
            "claude-code",
            "session",
            "turn",
            json!({
                "last_assistant_message": null,
                "agent_transcript_path": transcript.to_str().unwrap(),
            }),
        )
        .unwrap();

    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].actor(), Actor::Agent);
    assert_eq!(content(&events[0]), "subagent reply");
}

#[test]
fn a_prompt_opens_the_checkouts_turn_and_stop_closes_it() {
    let fixture = Fixture::new();
    let dir = turn_dir(&fixture.sessions, &fixture.root);

    let prompt = fixture.prompt("codex", "session", "", "hello");
    let open = TurnState::latest_cause(&dir).unwrap().unwrap();
    assert_eq!(open.as_uuid().to_string(), prompt);

    fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    assert_eq!(TurnState::latest_cause(&dir).unwrap(), None);
    assert_eq!(fixture.state_file_count(), 0);
}

#[test]
fn a_stop_leaves_a_later_prompts_turn_open() {
    let fixture = Fixture::new();
    let dir = turn_dir(&fixture.sessions, &fixture.root);
    fixture.prompt("codex", "first", "", "hello");
    let later = fixture.prompt("claude-code", "second", "", "hello");

    fixture
        .stop("codex", "first", "", json!({"last_assistant_message": "reply"}))
        .unwrap();

    let open = TurnState::latest_cause(&dir).unwrap().unwrap();
    assert_eq!(open.as_uuid().to_string(), later);
}

#[test]
fn a_session_start_closes_a_turn_a_killed_session_left_open() {
    let fixture = Fixture::new();
    let dir = turn_dir(&fixture.sessions, &fixture.root);
    fixture.prompt("codex", "killed", "", "hello");

    fixture.session_start("codex");

    assert_eq!(TurnState::latest_cause(&dir).unwrap(), None);
}

#[test]
fn checkouts_sessions_and_turns_keep_separate_causes() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    let other_root = fixture.other_root();

    let cases = [
        ("codex", root.clone(), "one", "a"),
        ("claude-code", root.clone(), "one", "a"),
        ("codex", other_root.clone(), "one", "a"),
        ("codex", root.clone(), "two", "a"),
        ("codex", root.clone(), "one", "b"),
    ];

    let prompts: Vec<String> = cases
        .iter()
        .enumerate()
        .map(|(index, (client, root, session, turn))| {
            fixture.prompt_at(client, root, session, turn, &index.to_string())
        })
        .collect();

    for (index, (client, root, session, turn)) in cases.iter().enumerate() {
        fixture
            .stop_at(
                client,
                root,
                session,
                turn,
                json!({"last_assistant_message": index.to_string()}),
            )
            .unwrap();
    }

    let replies: Vec<_> = fixture
        .events()
        .into_iter()
        .filter(|event| event.actor() == Actor::Agent)
        .collect();
    assert_eq!(replies.len(), cases.len());
    for reply in &replies {
        let index: usize = content(reply).parse().unwrap();
        assert_eq!(
            reply.causation_id().unwrap().as_uuid().to_string(),
            prompts[index]
        );
        assert_eq!(reply.source().name, cases[index].0);
        assert_eq!(reply.source().path, cases[index].1);
    }
}

#[test]
fn subdirectory_uses_the_checkout_root_and_its_existing_prompt() {
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "", "hello");
    let nested = fixture.root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();

    fixture
        .stop_at(
            "codex",
            &nested,
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert_eq!(reply.source().path, fixture.root);
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
}

#[test]
fn failed_prompt_removes_the_previous_cause() {
    let fixture = Fixture::new();
    fixture.prompt("codex", "session", "", "hello");
    fixture.log.start_failing();

    let err = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": fixture.root.to_str().unwrap(),
                "session_id": "session",
                "prompt": "next",
            }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("append failed"));

    fixture.log.stop_failing();
    fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert!(reply.causation_id().is_none());
}

#[test]
fn invalid_prompt_does_not_touch_the_previous_cause() {
    // A malformed `prompt` fails to parse before `run` ever opens the
    // turn's state, so - unlike a well-formed prompt whose commit fails -
    // the previous prompt's cause is left standing.
    let fixture = Fixture::new();
    let prompt = fixture.prompt("codex", "session", "", "hello");

    let err = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": fixture.root.to_str().unwrap(),
                "session_id": "session",
                "prompt": {},
            }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("invalid type: map, expected a string"));

    fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
}

#[test]
fn unknown_turn_does_not_inherit_another_turns_prompt() {
    let fixture = Fixture::new();
    fixture.prompt("codex", "session", "earlier", "hello");

    fixture
        .stop(
            "codex",
            "session",
            "later",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Agent)
        .unwrap();
    assert!(reply.causation_id().is_none());
}

#[test]
fn empty_reply_is_not_an_event() {
    let fixture = Fixture::new();
    fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": " \n"}),
        )
        .unwrap();

    assert!(fixture.events().is_empty());
}

#[test]
fn null_reply_is_not_an_error_or_event() {
    let fixture = Fixture::new();
    let output = fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": null}),
        )
        .unwrap();

    assert_eq!(output, json!({}));
    assert!(fixture.events().is_empty());
}

#[test]
fn an_unreadable_transcript_is_an_error_not_an_event() {
    let fixture = Fixture::new();
    let missing = fixture._temp.path().join("absent transcript.jsonl");

    let err = fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"transcript_path": missing.to_str().unwrap()}),
        )
        .unwrap_err();

    assert!(!err.to_string().is_empty());
    assert!(fixture.events().is_empty());
}

#[test]
fn claude_fallback_reads_only_the_current_turn_and_keeps_tool_results() {
    let fixture = Fixture::new();
    let transcript = fixture._temp.path().join("transcript.jsonl");
    let entries = [
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "old"}]}}),
        json!({"type": "user", "message": {"content": "current prompt"}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "start"}]}}),
        json!({"type": "user", "message": {"content": [{"type": "tool_result", "content": "ok"}]}}),
        json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "end"}]}}),
    ];
    let text = entries
        .iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&transcript, text).unwrap();

    fixture
        .stop(
            "claude-code",
            "session",
            "",
            json!({"transcript_path": transcript.to_str().unwrap(), "last_assistant_message": null}),
        )
        .unwrap();

    let events = fixture.events();
    assert_eq!(content(&events[0]), "start\nend");
}

#[test]
fn malformed_input_reports_an_error_without_blocking() {
    let fixture = Fixture::new();
    let err = fixture.call_raw("codex", "{broken").unwrap_err();
    assert!(!err.to_string().is_empty());
}
