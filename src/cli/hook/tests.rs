use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::core::testing::{content, human, node_id, FakeLog};
use crate::core::{HumanId, Payload};

/// A checkout `run` can discover a root in - a `.percept` marker is
/// enough, so a test needs no `git init` - plus the sessions directory
/// and log `run` is given. `run` now loads schemas itself, from this
/// same root, exactly as production does - decisions and tasks come
/// built in, free of any file; `with_extra_schema` adds a project one
/// for a test that needs a map shape the built-ins don't have.
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
        Self {
            sessions: temp.path().join("storage/hook-sessions"),
            root,
            log: FakeLog::default(),
            me: human(),
            _temp: temp,
        }
    }

    /// Writes `<root>/.percept/schemas/<name>.toml`, so the next
    /// `session_start` folds a project schema alongside the built-in
    /// decisions and tasks - the same file `load_schemas` reads in
    /// production.
    fn with_extra_schema(self, name: &str, toml: &str) -> Self {
        let dir = self.root.join(".percept/schemas");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.toml")), toml).unwrap();
        self
    }

    /// Another project, so a test can tell one checkout's cause from
    /// another's.
    fn other_root(&self) -> PathBuf {
        let other = self._temp.path().join("second checkout");
        std::fs::create_dir_all(other.join(".percept")).unwrap();
        other.canonicalize().unwrap()
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
        let root = crate::project_of(&checkout);
        let source = Source {
            name: client.to_string(),
            path: root,
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
    /// root, returning the `additionalContext` block.
    fn session_start(&self, client: &str) -> String {
        let output = self
            .call(
                client,
                json!({
                    "hook_event_name": "SessionStart",
                    "cwd": self.root.to_str().unwrap(),
                    "session_id": "session",
                }),
            )
            .unwrap();
        output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .to_string()
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

    fn events(&self) -> Vec<crate::core::Event> {
        self.log.load().unwrap()
    }

    /// Appends a `node.added` event for this fixture's own project,
    /// `at` a given moment - the moment a "gained since" test needs to
    /// control - and returns it so a test can point an edge at the
    /// node it minted.
    fn seed_node(&self, map: &str, kind: &str, name: &str, at: Timestamp) -> Event {
        self.seed_node_with_sources(map, kind, name, at, Vec::new())
    }

    /// Appends an `edge.added` event resolving `from` against `to`, at
    /// the same moment as `from` was recorded.
    fn seed_edge(&self, map: &str, kind: &str, from: &Event, to: &Event) {
        let event = Event::restore(
            EventId::new(),
            Actor::Human(human()),
            self.source(),
            None,
            from.created_at(),
            Payload::EdgeAdded {
                map: map.to_string(),
                kind: kind.to_string(),
                from: node_id(from),
                to: node_id(to),
                sources: Vec::new(),
            },
        );
        self.log.append(&event).unwrap();
    }

    /// Appends a `claim.confirmed` event for `node`, at `at`.
    fn seed_confirmed(&self, map: &str, node: &Event, at: Timestamp) {
        self.seed_judgment(
            Payload::ClaimConfirmed {
                map: map.to_string(),
                node: node_id(node),
            },
            at,
        );
    }

    /// Appends a `claim.disputed` event for `node`, with `why`, at `at`.
    fn seed_disputed(&self, map: &str, node: &Event, why: &str, at: Timestamp) {
        self.seed_judgment(
            Payload::ClaimDisputed {
                map: map.to_string(),
                node: node_id(node),
                why: why.to_string(),
            },
            at,
        );
    }

    /// Appends the human's judgment `payload` at `at` - the moment a
    /// "judged since" test needs to control.
    fn seed_judgment(&self, payload: Payload, at: Timestamp) {
        let event = Event::restore(
            EventId::new(),
            Actor::Human(self.me),
            self.source(),
            None,
            at,
            payload,
        );
        self.log.append(&event).unwrap();
    }

    /// Appends a `file.cited` event citing `path` (repo-relative)
    /// with `excerpt` as its text, `lines` the ranged read or `None`
    /// for the whole file, `at` the moment it was seen, caused by
    /// `causation` when this is a re-citation of an earlier one.
    fn seed_citation(
        &self,
        path: &str,
        lines: Option<(u32, u32)>,
        excerpt: &str,
        at: Timestamp,
        causation: Option<EventId>,
    ) -> Event {
        let event = Event::restore(
            EventId::new(),
            Actor::Agent,
            self.source(),
            causation,
            at,
            Payload::FileCited {
                path: PathBuf::from(path),
                lines,
                excerpt: excerpt.to_string(),
            },
        );
        self.log.append(&event).unwrap();
        event
    }

    /// `seed_node`, but with `sources` set - the ids a node cites, as a
    /// `changed since recorded` test needs to point one at a
    /// file.cited event.
    fn seed_node_with_sources(
        &self,
        map: &str,
        kind: &str,
        name: &str,
        at: Timestamp,
        sources: Vec<EventId>,
    ) -> Event {
        self.seed_node_by(Actor::Human(human()), map, kind, name, at, sources)
    }

    /// `seed_node`, but written by the model - a judgment test needs
    /// one, since a user-written node carries no standing to judge.
    fn seed_agent_node(&self, map: &str, kind: &str, name: &str, at: Timestamp) -> Event {
        self.seed_node_by(Actor::Agent, map, kind, name, at, Vec::new())
    }

    fn seed_node_by(
        &self,
        actor: Actor,
        map: &str,
        kind: &str,
        name: &str,
        at: Timestamp,
        sources: Vec<EventId>,
    ) -> Event {
        let event = Event::restore(
            EventId::new(),
            actor,
            self.source(),
            None,
            at,
            Payload::NodeAdded {
                map: map.to_string(),
                node: crate::core::NodeId::new(),
                kind: kind.to_string(),
                name: name.to_string(),
                properties: std::collections::BTreeMap::new(),
                sources,
                seq: 0,
            },
        );
        self.log.append(&event).unwrap();
        event
    }

    /// The source every seeded event carries: this fixture's project,
    /// written by codex.
    fn source(&self) -> Source {
        Source {
            name: "codex".to_string(),
            path: self.root.clone(),
        }
    }

    /// When the previous session started - what a returning session's
    /// "since" is, after one `session_start` has run.
    fn since(&self) -> Timestamp {
        self.events()
            .into_iter()
            .find(|event| matches!(event.payload(), Payload::SessionStarted))
            .unwrap()
            .created_at()
    }

    /// Writes `<root>/<path>`, creating any missing parent directories -
    /// the working tree a `changed since recorded` test checks against.
    fn write_file(&self, path: &str, content: &str) {
        let full = self.root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, content).unwrap();
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
fn a_broken_project_schema_does_not_stop_prompt_capture() {
    // Only `SessionStart` needs schemas at all; a project's own
    // `.percept/schemas/*.toml` failing to load must not also break
    // recording an ordinary prompt, which reads no schema.
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
fn a_first_session_here_records_the_marker_and_says_so() {
    let fixture = Fixture::new();
    let context = fixture.session_start("codex");

    assert!(context.contains("first session here"), "{context:?}");
    let events = fixture.events();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0].payload(), Payload::SessionStarted));
    assert_eq!(events[0].actor(), Actor::System);
}

#[test]
fn a_returning_session_reports_since_the_previous_one() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let context = fixture.session_start("codex");

    assert!(context.contains("since your last session here"), "{context:?}");
    assert!(!context.contains("first session here"), "{context:?}");
    assert_eq!(fixture.events().len(), 2);
}

#[test]
fn a_different_client_or_project_sees_its_own_first_session() {
    let fixture = Fixture::new();
    fixture.session_start("codex");

    assert!(fixture.session_start("claude-code").contains("first session here"));

    let other = fixture.other_root();
    let output = fixture
        .call(
            "codex",
            json!({
                "hook_event_name": "SessionStart",
                "cwd": other.to_str().unwrap(),
                "session_id": "session",
            }),
        )
        .unwrap();
    let context = output["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("first session here"), "{context:?}");
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
fn stop_clears_the_turns_state() {
    let fixture = Fixture::new();
    fixture.prompt("codex", "session", "", "hello");
    assert_eq!(fixture.state_file_count(), 1);

    fixture
        .stop(
            "codex",
            "session",
            "",
            json!({"last_assistant_message": "reply"}),
        )
        .unwrap();

    assert_eq!(fixture.state_file_count(), 0);
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

/// A project schema with no `Settlement` - an `ideas` map, say - so a
/// test can prove the open section is skipped by reading the schema,
/// never by the map's name.
const IDEAS_TOML: &str = r#"
name = "ideas"
purpose = "loose thoughts worth keeping"
headlines = ["idea"]

[[nodes]]
name = "idea"
gloss = "a loose thought"
"#;

#[test]
fn a_first_session_with_history_shows_open_items_but_no_gained_section() {
    let fixture = Fixture::new();
    fixture.seed_node("decisions", "question", "why blue?", Timestamp::now());
    fixture.seed_node("tasks", "task", "ship it", Timestamp::now());

    let context = fixture.session_start("codex");

    assert!(context.contains("open question (1)"), "{context:?}");
    assert!(context.contains("open task (1)"), "{context:?}");
    assert!(!context.contains("decisions +"), "{context:?}");
    assert!(!context.contains("tasks +"), "{context:?}");
}

#[test]
fn an_open_question_that_reopens_a_decision_says_which() {
    let fixture = Fixture::new();
    let at = Timestamp::now();
    let question = fixture.seed_node("decisions", "question", "which parser?", at);
    let decision = fixture.seed_node("decisions", "decision", "its own", at);
    fixture.seed_edge("decisions", "resolves", &decision, &question);
    let doubt = fixture.seed_node("decisions", "question", "does its own still fit?", at);
    fixture.seed_edge("decisions", "reopens", &doubt, &decision);

    let context = fixture.session_start("codex");

    assert!(context.contains("q2 \"does its own still fit?\" reopens d1"), "{context:?}");
    assert!(context.contains("open question (1)"), "{context:?}");
}

#[test]
fn every_session_start_ends_with_the_recording_rules() {
    let fixture = Fixture::new();

    let first = fixture.session_start("codex");
    let returning = fixture.session_start("codex");

    for context in [first, returning] {
        let rules = context.rsplit("\n\n").next().unwrap();
        assert!(rules.starts_with("recording\n"), "{context:?}");
        assert!(rules.contains("percept maps record decisions --actor agent --source <prompt id>"));
        assert!(rules.contains("Recorded to decisions:"));
    }
}

#[test]
fn a_returning_session_reports_counts_and_excludes_older_gains() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    fixture.seed_node("decisions", "question", "an older one", since.minus_minutes(60).unwrap());
    fixture.seed_node(
        "decisions",
        "question",
        "a fresh one",
        since.minus_minutes(-60).unwrap(),
    );

    let context = fixture.session_start("codex");

    assert!(context.contains("decisions +1"), "{context:?}");
    assert!(context.contains("tasks +0"), "{context:?}");
    // The gained line for a question names its kind; the open section's
    // line for the same node does not, so this pattern only ever comes
    // from the gained block.
    assert!(context.contains("question \"a fresh one\""), "{context:?}");
    assert!(!context.contains("question \"an older one\""), "{context:?}");
}

#[test]
fn a_node_disputed_after_the_previous_session_shows_its_why() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    let option = fixture.seed_agent_node(
        "decisions",
        "option",
        "a terminal render of the since-cut, the page later",
        since.minus_minutes(120).unwrap(),
    );
    fixture.seed_disputed(
        "decisions",
        &option,
        "never proposed",
        since.minus_minutes(-30).unwrap(),
    );

    let context = fixture.session_start("codex");

    assert!(context.contains("judged since your last session (1)"), "{context:?}");
    assert!(
        context.contains(
            "option \"a terminal render of the since-cut, the page later\" \u{b7} disputed: \"never proposed\""
        ),
        "{context:?}"
    );
}

#[test]
fn a_node_confirmed_shows_confirmed_with_no_why() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    let decision = fixture.seed_agent_node(
        "decisions",
        "decision",
        "percept review serves a local web page",
        since.minus_minutes(120).unwrap(),
    );
    fixture.seed_confirmed("decisions", &decision, since.minus_minutes(-30).unwrap());

    let context = fixture.session_start("codex");

    assert!(
        context.contains("decision \"percept review serves a local web page\" \u{b7} confirmed"),
        "{context:?}"
    );
    assert!(!context.contains("confirmed: "), "{context:?}");
}

#[test]
fn a_judgment_from_before_the_previous_session_is_not_shown() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    let decision = fixture.seed_agent_node(
        "decisions",
        "decision",
        "an old decision",
        since.minus_minutes(120).unwrap(),
    );
    fixture.seed_confirmed("decisions", &decision, since.minus_minutes(90).unwrap());

    let context = fixture.session_start("codex");

    assert!(!context.contains("judged since your last session"), "{context:?}");
}

#[test]
fn disputed_lines_come_before_confirmed_ones() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    let confirmed = fixture.seed_agent_node(
        "decisions",
        "decision",
        "confirmed one",
        since.minus_minutes(120).unwrap(),
    );
    fixture.seed_confirmed("decisions", &confirmed, since.minus_minutes(-10).unwrap());
    let disputed = fixture.seed_agent_node(
        "decisions",
        "option",
        "disputed one",
        since.minus_minutes(120).unwrap(),
    );
    fixture.seed_disputed(
        "decisions",
        &disputed,
        "why not",
        since.minus_minutes(-20).unwrap(),
    );

    let context = fixture.session_start("codex");

    let judged = context.split("judged since your last session").nth(1).unwrap();
    let disputed_at = judged.find("disputed one").unwrap();
    let confirmed_at = judged.find("confirmed one").unwrap();
    assert!(disputed_at < confirmed_at, "{context:?}");
}

#[test]
fn no_judgment_means_no_judged_since_block() {
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();
    fixture.seed_node(
        "decisions",
        "question",
        "unjudged",
        since.minus_minutes(-10).unwrap(),
    );

    let context = fixture.session_start("codex");

    assert!(!context.contains("judged since your last session"), "{context:?}");
}

#[test]
fn a_first_session_prints_no_judged_block_even_with_judgments() {
    let fixture = Fixture::new();
    let decision = fixture.seed_agent_node("decisions", "decision", "a decision", Timestamp::now());
    fixture.seed_confirmed("decisions", &decision, Timestamp::now());

    let context = fixture.session_start("codex");

    assert!(context.contains("first session here"), "{context:?}");
    assert!(!context.contains("judged since your last session"), "{context:?}");
}

#[test]
fn resolving_an_old_question_does_not_report_it_as_gained() {
    // `Map::since` would also surface `question` here, since a fresh
    // `resolves` edge touches it; the gained block compares `added_at`
    // directly instead, so only the decision itself counts as new.
    let fixture = Fixture::new();
    fixture.session_start("codex");
    let since = fixture.since();

    let question = fixture.seed_node(
        "decisions",
        "question",
        "an old question",
        since.minus_minutes(120).unwrap(),
    );
    let decision = fixture.seed_node(
        "decisions",
        "decision",
        "a fresh decision",
        since.minus_minutes(-30).unwrap(),
    );
    fixture.seed_edge("decisions", "resolves", &decision, &question);

    let context = fixture.session_start("codex");

    assert!(context.contains("decisions +1"), "{context:?}");
    assert!(context.contains("decision \"a fresh decision\""), "{context:?}");
    assert!(!context.contains("an old question"), "{context:?}");
}

#[test]
fn more_than_five_open_items_shows_five_and_a_correct_more_count() {
    let fixture = Fixture::new();
    for n in 1..=6 {
        fixture.seed_node("decisions", "question", &format!("question {n}"), Timestamp::now());
    }

    let context = fixture.session_start("codex");

    assert!(context.contains("open question (6, showing 5)"), "{context:?}");
    for n in 1..=5 {
        assert!(context.contains(&format!("question {n}")), "{context:?}");
    }
    assert!(!context.contains("question 6"), "{context:?}");
    assert!(context.contains("+1 more"), "{context:?}");
}

#[test]
fn a_settled_question_s_decision_does_not_itself_count_as_open() {
    // decisions' headline_kinds is ["question", "decision"], and
    // nothing ever resolves a decision node itself - only the settled
    // kind (`settlement.of`, "question" here) can be open, or every
    // decision misreports as one.
    let fixture = Fixture::new();
    let question = fixture.seed_node("decisions", "question", "settled one", Timestamp::now());
    let decision = fixture.seed_node("decisions", "decision", "the answer", Timestamp::now());
    fixture.seed_edge("decisions", "resolves", &decision, &question);
    fixture.seed_node("decisions", "question", "still open", Timestamp::now());

    let context = fixture.session_start("codex");

    assert!(context.contains("open question (1)"), "{context:?}");
    assert!(context.contains("still open"), "{context:?}");
    assert!(!context.contains("settled one"), "{context:?}");
    assert!(!context.contains("the answer"), "{context:?}");
}

#[test]
fn a_map_without_settlement_has_no_open_section() {
    let fixture = Fixture::new().with_extra_schema("ideas", IDEAS_TOML);
    fixture.seed_node("ideas", "idea", "a loose thought", Timestamp::now());
    fixture.seed_node("decisions", "question", "settled how?", Timestamp::now());

    let context = fixture.session_start("codex");

    assert!(context.contains("open question (1)"), "{context:?}");
    assert!(!context.contains("open idea"), "{context:?}");
}

#[test]
fn an_unchanged_seen_file_reports_nothing() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn one() {}\nfn two() {}\n");
    let citation = fixture.seed_citation(
        "src/a.rs",
        Some((1, 1)),
        "fn one() {}",
        Timestamp::now(),
        None,
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn an_edited_range_reports_changed() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn one() { edited }\n");
    let citation = fixture.seed_citation(
        "src/a.rs",
        Some((1, 1)),
        "fn one() {}",
        Timestamp::now(),
        None,
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(context.contains("changed since recorded"), "{context:?}");
    assert!(context.contains("src/a.rs:1-1 changed"), "{context:?}");
}

#[test]
fn a_deleted_file_reports_gone() {
    let fixture = Fixture::new();
    let citation = fixture.seed_citation(
        "src/missing.rs",
        None,
        "fn gone() {}",
        Timestamp::now(),
        None,
    );
    fixture.seed_node_with_sources(
        "tasks",
        "task",
        "fix it",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(context.contains("changed since recorded"), "{context:?}");
    assert!(context.contains("src/missing.rs gone"), "{context:?}");
}

#[test]
fn text_moved_to_other_lines_reports_nothing() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn zero() {}\nfn one() {}\n");
    let citation = fixture.seed_citation(
        "src/a.rs",
        Some((5, 5)),
        "fn one() {}",
        Timestamp::now(),
        None,
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn a_re_citation_replaces_the_one_checked() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn two() {}\n");
    let first = fixture.seed_citation(
        "src/a.rs",
        Some((1, 1)),
        "fn one() {}",
        Timestamp::now().minus_minutes(10).unwrap(),
        None,
    );
    let second = fixture.seed_citation(
        "src/a.rs",
        Some((1, 1)),
        "fn two() {}",
        Timestamp::now(),
        Some(first.id()),
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![first.id()],
    );
    let _ = second;

    let context = fixture.session_start("codex");

    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn a_superseded_decisions_seen_file_is_not_checked() {
    let fixture = Fixture::new();
    let citation = fixture.seed_citation(
        "src/gone.rs",
        None,
        "fn gone() {}",
        Timestamp::now(),
        None,
    );
    let old = fixture.seed_node_with_sources(
        "decisions",
        "decision",
        "old answer",
        Timestamp::now(),
        vec![citation.id()],
    );
    let new = fixture.seed_node_with_sources(
        "decisions",
        "decision",
        "new answer",
        Timestamp::now(),
        Vec::new(),
    );
    fixture.seed_edge("decisions", "supersedes", &new, &old);

    let context = fixture.session_start("codex");

    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn pointer_names_first_open_item_in_schema_fold_order() {
    let fixture = Fixture::new();
    fixture.seed_node(
        "tasks",
        "task",
        "an older task",
        Timestamp::now().minus_minutes(60).unwrap(),
    );
    fixture.seed_node("decisions", "question", "a newer question", Timestamp::now());

    let context = fixture.session_start("codex");

    let pointer = context.lines().find(|line| line.starts_with("fragment:")).unwrap();
    assert!(
        pointer.contains("percept maps show decisions --around"),
        "{pointer:?}"
    );
}

#[test]
fn a_later_citation_of_a_different_path_does_not_replace_the_one_checked() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn one() {}\n");
    let first = fixture.seed_citation(
        "src/a.rs",
        None,
        "fn one() {}",
        Timestamp::now().minus_minutes(10).unwrap(),
        None,
    );
    // Caused by `first`, but a different path - not a re-citation of
    // `src/a.rs`, so it must not stand in for it.
    fixture.seed_citation(
        "src/b.rs",
        None,
        "fn two() {}",
        Timestamp::now(),
        Some(first.id()),
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![first.id()],
    );

    let context = fixture.session_start("codex");

    // `src/a.rs` still reads as recorded, so nothing changed.
    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn a_cited_file_with_one_invalid_utf8_byte_elsewhere_still_reads() {
    let fixture = Fixture::new();
    let mut bytes = b"fn one() {}\n// ".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b"\n");
    let full = fixture.root.join("src/a.rs");
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, &bytes).unwrap();
    let citation = fixture.seed_citation(
        "src/a.rs",
        Some((1, 1)),
        "fn one() {}",
        Timestamp::now(),
        None,
    );
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(!context.contains("changed since recorded"), "{context:?}");
}

#[test]
fn a_citation_whose_excerpt_is_blank_reports_changed() {
    let fixture = Fixture::new();
    fixture.write_file("src/a.rs", "fn one() {}\n");
    let citation = fixture.seed_citation("src/a.rs", None, "   \n  ", Timestamp::now(), None);
    fixture.seed_node_with_sources(
        "decisions",
        "question",
        "why a?",
        Timestamp::now(),
        vec![citation.id()],
    );

    let context = fixture.session_start("codex");

    assert!(context.contains("changed since recorded"), "{context:?}");
    assert!(context.contains("src/a.rs changed"), "{context:?}");
}
