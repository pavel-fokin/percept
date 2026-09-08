use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::core::testing::{content, FakeLog};
use crate::core::Payload;

/// A checkout `run` can discover a root in - a `.percept` marker is
/// enough, so a test needs no `git init` - plus the sessions directory
/// and log `run` is given.
struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    sessions: PathBuf,
    log: FakeLog,
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
        Self {
            sessions: temp.path().join("storage/hook-sessions"),
            root,
            log: FakeLog::default(),
            _temp: temp,
        }
    }

    /// Another project, so a test can tell one checkout's cause from
    /// another's.
    fn other_root(&self) -> PathBuf {
        let other = self._temp.path().join("second checkout");
        std::fs::create_dir_all(other.join(".percept")).unwrap();
        other.canonicalize().unwrap()
    }

    fn call(&self, client: &str, body: Value) -> Result<Value, Box<dyn std::error::Error>> {
        let mut input = Cursor::new(body.to_string().into_bytes());
        run(client, &mut input, &self.log, &self.sessions)
    }

    fn call_raw(&self, client: &str, raw: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let mut input = Cursor::new(raw.as_bytes().to_vec());
        run(client, &mut input, &self.log, &self.sessions)
    }

    fn prompt(&self, client: &str, root: &Path, session: &str, turn: &str, text: &str) -> String {
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
            .strip_prefix("percept event ")
            .unwrap()
            .to_string()
    }

    fn stop(
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

    fn events(&self) -> Vec<crate::core::Event> {
        self.log.load().unwrap()
    }
}

fn merge(base: &mut Value, extra: Value) {
    if let (Some(base), Some(extra)) = (base.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
}

#[test]
fn prompt_context_names_the_committed_event() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    let id = fixture.prompt("codex", &root, "session", "", "hello");

    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].id().as_uuid().to_string(), id);
    assert_eq!(content(&events[0]), "hello");
    assert!(events[0].actor() == Actor::User);
}

#[test]
fn any_client_name_becomes_the_events_source() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.prompt("opencode", &root, "session", "", "hello");

    assert_eq!(fixture.events()[0].source().name, "opencode");
}

#[test]
fn an_empty_client_name_is_refused_without_blocking() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    let err = fixture
        .call(
            "",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": root.to_str().unwrap(),
                "session_id": "session",
                "prompt": "hello",
            }),
        )
        .unwrap_err();

    assert!(err.to_string().contains("client name must not be empty"));
    assert!(fixture.events().is_empty());
}

#[test]
fn tool_result_cites_the_call_and_the_call_cites_the_prompt() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    let prompt = fixture.prompt("codex", &root, "session", "", "hello");

    fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "PostToolUse",
                "cwd": root.to_str().unwrap(),
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
    let root = fixture.root.clone();
    let prompt = fixture.prompt("codex", &root, "session", "", "hello");

    let output = fixture
        .stop(
            "codex",
            &root,
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    assert_eq!(output, json!({}));
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Model)
        .unwrap();
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
    assert_eq!(content(&reply), "reply");
}

#[test]
fn stop_clears_the_turns_state() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.prompt("codex", &root, "session", "", "hello");
    assert_eq!(std::fs::read_dir(&fixture.sessions).unwrap().count(), 1);

    fixture
        .stop(
            "codex",
            &root,
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    assert_eq!(std::fs::read_dir(&fixture.sessions).unwrap().count(), 0);
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
            fixture.prompt(client, root, session, turn, &index.to_string())
        })
        .collect();

    for (index, (client, root, session, turn)) in cases.iter().enumerate() {
        fixture
            .stop(
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
        .filter(|event| event.actor() == Actor::Model)
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
    let root = fixture.root.clone();
    let prompt = fixture.prompt("codex", &root, "session", "", "hello");
    let nested = root.join("nested");
    std::fs::create_dir_all(&nested).unwrap();

    fixture
        .stop(
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
        .find(|event| event.actor() == Actor::Model)
        .unwrap();
    assert_eq!(reply.source().path, root);
    assert_eq!(reply.causation_id().unwrap().as_uuid().to_string(), prompt);
}

#[test]
fn failed_prompt_removes_the_previous_cause() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.prompt("codex", &root, "session", "", "hello");
    fixture.log.start_failing();

    let err = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": root.to_str().unwrap(),
                "session_id": "session",
                "prompt": "next",
            }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("append failed"));

    // The failed append still ran through the fake log's failure
    // switch, so flip it back before the turn's `Stop` tries to append
    // the reply.
    let log = FakeLog::default();
    let fixture = Fixture {
        log,
        ..fixture
    };
    fixture
        .stop(
            "codex",
            &root,
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Model)
        .unwrap();
    assert!(reply.causation_id().is_none());
}

#[test]
fn invalid_prompt_removes_the_previous_cause() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.prompt("codex", &root, "session", "", "hello");

    let err = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "UserPromptSubmit",
                "cwd": root.to_str().unwrap(),
                "session_id": "session",
                "prompt": {},
            }),
        )
        .unwrap_err();
    assert!(err.to_string().contains("prompt must be a string"));

    fixture
        .stop(
            "codex",
            &root,
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Model)
        .unwrap();
    assert!(reply.causation_id().is_none());
}

#[test]
fn unknown_turn_does_not_inherit_another_turns_prompt() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture.prompt("codex", &root, "session", "earlier", "hello");

    fixture
        .stop(
            "codex",
            &root,
            "session",
            "later",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();
    let reply = fixture
        .events()
        .into_iter()
        .find(|event| event.actor() == Actor::Model)
        .unwrap();
    assert!(reply.causation_id().is_none());
}

#[test]
fn empty_reply_is_not_an_event() {
    let fixture = Fixture::new();
    let root = fixture.root.clone();
    fixture
        .stop(
            "codex",
            &root,
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
    let root = fixture.root.clone();
    let output = fixture
        .stop(
            "codex",
            &root,
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
    let root = fixture.root.clone();
    let missing = fixture._temp.path().join("absent transcript.jsonl");

    let err = fixture
        .stop(
            "codex",
            &root,
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
    let root = fixture.root.clone();
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
            &root,
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
