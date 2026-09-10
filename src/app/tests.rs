use super::*;
use crate::core::testing::{content, human, node_added, schemas, scope, source, usage, FakeLog};
use crate::core::{Actor, Payload};
use crate::harness::testing::{FakeCatalog, FakeSnapshot, FakeTool, FixedPolicy, Scripted};
use crate::harness::{Chunk, Verdict};

const SOURCE: &str = "tui";

fn thought(event: &Event) -> &str {
    match event.payload() {
        Payload::ThoughtRecorded { content } => content,
        _ => panic!("expected a thought.recorded event"),
    }
}

struct Silent;

impl crate::harness::Model for Silent {
    fn capabilities(&self) -> crate::harness::ModelCapabilities {
        crate::harness::ModelCapabilities {
            input: &[crate::harness::Modality::Text],
            output: &[crate::harness::Modality::Text],
            tool_use: false,
            reasoning_efforts: &[],
            default_reasoning_effort: None,
            context_window: None,
        }
    }

    fn name(&self) -> &str {
        "silent"
    }

    fn reply(&self, _request: &crate::harness::ModelRequest) -> crate::harness::ReplyStream {
        Box::pin(tokio_stream::empty())
    }
}

#[test]
fn streamed_reply_commits_one_event_caused_by_the_prompt() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    assert_eq!(app.events().len(), 1);
    assert!(app.pending_reply().is_none());

    app.append_chunk(Chunk::Reply("he".to_string()));
    app.append_chunk(Chunk::Reply("llo".to_string()));
    assert_eq!(app.pending_reply(), Some("hello"));
    assert_eq!(app.events().len(), 1);

    app.end_stream().unwrap();
    assert!(app.pending_reply().is_none());

    let events = app.events();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0].actor(), Actor::Human(_)));
    assert!(events[1].actor() == Actor::Agent);
    assert_eq!(content(&events[1]), "hello");
    assert!(events[1].causation_id() == Some(events[0].id()));
    assert_eq!(events[0].source().name, SOURCE);
    assert_eq!(events[1].source().name, SOURCE);
}

#[test]
fn a_thought_and_a_reply_commit_as_two_model_events_thought_first() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Thought("hmm".to_string()));
    app.append_chunk(Chunk::Reply("hello".to_string()));
    assert_eq!(app.pending_thought(), Some("hmm"));
    assert_eq!(app.pending_reply(), Some("hello"));

    app.end_stream().unwrap();
    assert!(app.pending_thought().is_none());
    assert!(app.pending_reply().is_none());

    let events = app.events();
    assert_eq!(events.len(), 3);
    assert!(events[1].actor() == Actor::Agent);
    assert_eq!(thought(&events[1]), "hmm");
    assert!(events[2].actor() == Actor::Agent);
    assert_eq!(content(&events[2]), "hello");
    assert!(events[1].causation_id() == Some(events[0].id()));
    assert!(events[2].causation_id() == Some(events[0].id()));
}

#[test]
fn a_plain_turn_commits_thought_reply_then_model_called_in_that_order() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Thought("hmm".to_string()));
    app.append_chunk(Chunk::Reply("hello".to_string()));
    app.append_chunk(Chunk::Usage(usage()));
    app.end_stream().unwrap();

    let events = app.events();
    assert_eq!(events.len(), 4);
    assert!(matches!(
        events[1].payload(),
        Payload::ThoughtRecorded { .. }
    ));
    assert!(matches!(
        events[2].payload(),
        Payload::MessageReceived { .. }
    ));
    match events[3].payload() {
        Payload::ModelCalled(recorded) => assert_eq!(recorded, &usage()),
        _ => panic!("expected a model.called event"),
    }
    assert!(events[3].actor() == Actor::System);
    assert!(events[3].causation_id() == Some(events[0].id()));
}

#[test]
fn a_submit_while_a_turn_streams_is_refused_and_records_nothing() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("first".to_string()).unwrap();
    assert!(app.is_replying());
    assert!(app.submit("second".to_string()).is_err());
    assert_eq!(app.events().len(), 1);

    app.append_chunk(Chunk::Reply("done".to_string()));
    app.end_stream().unwrap();
    assert!(!app.is_replying());
    assert!(app.submit("second".to_string()).is_ok());
}

#[test]
fn a_turn_with_a_thought_and_no_reply_still_ends() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Thought("hm".to_string()));
    app.end_stream().unwrap();

    assert!(!app.is_replying());
    assert_eq!(app.events().len(), 2);
}

#[test]
fn empty_reply_commits_nothing() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    let _ = app.submit("hi".to_string()).unwrap();
    app.end_stream().unwrap();
    assert_eq!(app.events().len(), 1);
}

#[test]
fn preseeded_log_becomes_the_opening_transcript() {
    let seeded = vec![
        Event::message_received(Actor::Human(human()), "hi".to_string(), source(SOURCE), None),
        Event::message_received(Actor::Agent, "hello".to_string(), source(SOURCE), None),
    ];
    let log = Arc::new(FakeLog::seeded(seeded));
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    assert_eq!(app.events().len(), 2);

    let _ = app.submit("next".to_string()).unwrap();
    assert_eq!(app.events().len(), 3);
}

#[test]
fn another_source_s_conversation_in_the_same_project_stays_out_of_the_transcript() {
    let seeded = vec![
        Event::message_received(Actor::Human(human()), "hi".to_string(), source(SOURCE), None),
        Event::message_received(
            Actor::Human(human()),
            "codex was here".to_string(),
            source("codex"),
            None,
        ),
    ];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    assert_eq!(app.events().len(), 1);
    assert_eq!(content(&app.events()[0]), "hi");
}

#[test]
fn another_source_s_map_mutation_in_the_same_project_still_folds() {
    let seeded = vec![Event::new(
        Actor::Agent,
        source("codex"),
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: crate::core::NodeId::new(),
            kind: "question".to_string(),
            name: "why?".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
            seq: 1,
        },
    )];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let decisions = schemas()
        .fold_all(&scope(), app.events())
        .unwrap()
        .into_iter()
        .find(|map| map.schema().name == "decisions")
        .unwrap();
    assert_eq!(decisions.nodes().len(), 1);
}

#[test]
fn a_reopened_log_s_last_model_called_seeds_last_usage() {
    let seeded = vec![
        Event::message_received(Actor::Human(human()), "hi".to_string(), source(SOURCE), None),
        Event::model_called(usage(), source(SOURCE), None),
    ];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    assert_eq!(app.last_usage().unwrap(), &usage());
}

#[test]
fn a_log_with_no_model_called_leaves_last_usage_unset() {
    let seeded = vec![Event::message_received(
        Actor::Human(human()),
        "hi".to_string(),
        source(SOURCE),
        None,
    )];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    assert!(app.last_usage().is_none());
}

#[test]
fn append_failure_surfaces_as_err_and_leaves_transcript_unchanged() {
    let log = Arc::new(FakeLog::default());
    log.start_failing();
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    assert!(app.submit("hi".to_string()).is_err());
    assert!(app.events().is_empty());
}

#[test]
fn a_failed_reply_append_leaves_the_reply_pending() {
    let log = Arc::new(FakeLog::default());
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Reply("hello".to_string()));
    log.start_failing();

    assert!(app.end_stream().is_err());
    assert_eq!(app.pending_reply(), Some("hello"));
    assert_eq!(app.events().len(), 1);
}

#[test]
fn a_failed_thought_append_leaves_the_reply_unattempted() {
    let log = Arc::new(FakeLog::default());
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Thought("hmm".to_string()));
    app.append_chunk(Chunk::Reply("hello".to_string()));
    log.start_failing();

    assert!(app.end_stream().is_err());
    assert_eq!(app.pending_thought(), Some("hmm"));
    assert_eq!(app.pending_reply(), Some("hello"));
    assert_eq!(app.events().len(), 1);
}

/// What tui::handle_stream does for one tool call: begin, then act
/// on the `ToolStep` (running the tool inline instead of off-thread).
fn run_one_tool(app: &mut App, name: &str, arguments: &str) {
    match app.begin_tool(name, arguments.to_string()).unwrap() {
        ToolStep::Run(tool, args) => {
            let output = run_tool(&*tool, &args);
            let _ = app.finish_tool(output).unwrap();
        }
        // `begin_tool` already committed the result (no such tool)
        // or ended the turn (cap spent).
        ToolStep::Continue(_) | ToolStep::Stop => {}
        ToolStep::Ask(..) => panic!("the default policy never asks"),
    }
}

fn app_with_policy(policy: Verdict) -> App {
    App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness {
            policy: Arc::new(FixedPolicy(policy)),
            ..Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt)
        },
        source(SOURCE),
        human(),
    )
    .unwrap()
}

fn result_content(event: &Event) -> &str {
    match event.payload() {
        Payload::ToolResulted { content } => content,
        _ => panic!("expected a tool.resulted event"),
    }
}

#[test]
fn a_call_the_policy_asks_about_comes_back_as_an_ask_step_after_tool_called() {
    let mut app = app_with_policy(Verdict::Ask);

    let _ = app.submit("go".to_string()).unwrap();
    let step = app.begin_tool("search_events", "{}".to_string()).unwrap();

    assert!(matches!(step, ToolStep::Ask(_, ref args) if args == "{}"));
    // The call is on the record; nothing has run and no result exists.
    assert_eq!(app.events().len(), 2);
    assert!(matches!(
        app.events()[1].payload(),
        Payload::ToolCalled { .. }
    ));
}

#[test]
fn declining_an_asked_call_commits_the_refusal_as_its_result_and_asks_again() {
    let mut app = app_with_policy(Verdict::Ask);

    let _ = app.submit("go".to_string()).unwrap();
    let _ = app.begin_tool("search_events", "{}".to_string()).unwrap();
    let _ = app.decline_tool().unwrap();

    let events = app.events();
    assert_eq!(events.len(), 3);
    assert!(result_content(&events[2]).starts_with("The user declined to run search_events"));
    assert!(events[2].causation_id() == Some(events[1].id()));
    assert!(app.is_replying());
}

#[test]
fn a_tool_the_user_allowed_runs_unasked_for_the_rest_of_the_session() {
    let mut app = app_with_policy(Verdict::Ask);
    app.allow_tool("search_events");

    let _ = app.submit("go".to_string()).unwrap();
    let step = app.begin_tool("search_events", "{}".to_string()).unwrap();

    assert!(matches!(step, ToolStep::Run(..)));
}

#[test]
fn tool_progress_counts_the_turn_s_calls_against_the_cap_and_clears_between_turns() {
    let mut app = app_with_policy(Verdict::Allow);
    assert_eq!(app.tool_progress(), None);

    let _ = app.submit("go".to_string()).unwrap();
    assert_eq!(app.tool_progress(), Some((0, MAX_TOOL_CALLS)));
    run_one_tool(&mut app, "search_events", "{}");
    assert_eq!(app.tool_progress(), Some((1, MAX_TOOL_CALLS)));

    app.end_stream().unwrap();
    assert_eq!(app.tool_progress(), None);
}

#[test]
fn an_unknown_tool_is_refused_before_the_policy_is_asked() {
    let mut app = app_with_policy(Verdict::Ask);

    let _ = app.submit("go".to_string()).unwrap();
    let step = app.begin_tool("nope", "{}".to_string()).unwrap();

    assert!(matches!(step, ToolStep::Continue(_)));
    assert_eq!(result_content(&app.events()[2]), "no such tool: nope");
}

fn app_with_snapshot() -> (Arc<FakeSnapshot>, App) {
    let snapshot = Arc::new(FakeSnapshot::default());
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness {
            snapshot: Some(snapshot.clone()),
            ..Harness::new(Vec::new(), MapShape::Prompt)
        },
        source(SOURCE),
        human(),
    )
    .unwrap();
    (snapshot, app)
}

#[test]
fn a_turn_saves_the_tree_under_its_prompt_before_asking_the_model() {
    let (snapshot, mut app) = app_with_snapshot();

    let _ = app.submit("change it".to_string()).unwrap();

    assert_eq!(snapshot.taken(), vec![app.events()[0].id()]);
}

#[test]
fn undo_restores_the_last_turn_s_snapshot_once() {
    let (snapshot, mut app) = app_with_snapshot();
    let _ = app.submit("change it".to_string()).unwrap();
    app.end_stream().unwrap();
    let prompt = app.events()[0].id();

    app.undo().unwrap();
    assert_eq!(snapshot.restored(), vec![prompt]);

    assert_eq!(app.undo().unwrap_err().to_string(), "nothing to undo");
    assert_eq!(snapshot.restored(), vec![prompt]);
}

#[test]
fn undo_is_refused_while_a_turn_streams() {
    let (snapshot, mut app) = app_with_snapshot();
    let _ = app.submit("change it".to_string()).unwrap();

    assert!(app.undo().is_err());
    assert!(snapshot.restored().is_empty());
}

#[test]
fn an_app_without_a_snapshot_takes_none_and_cannot_undo() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    let _ = app.submit("hi".to_string()).unwrap();
    app.end_stream().unwrap();

    assert!(app.undo().is_err());
}

#[test]
fn instructions_go_to_the_model_as_system_text_before_the_maps_every_round() {
    let model = Arc::new(Scripted::new(vec![], true));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness {
            instructions: Some("Commit subjects stay under 72 chars.".to_string()),
            ..Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt)
        },
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");

    let sent = model.last_request();
    // The instructions, then the decisions map; the time goes last.
    assert!(sent[0].contains("Commit subjects stay under 72 chars."));
    assert!(sent[1].starts_with("The decisions map"));
}

#[test]
fn an_app_without_instructions_sends_none() {
    let model = Arc::new(Scripted::new(vec![], false));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();

    assert!(model
        .last_request()
        .iter()
        .all(|message| !message.contains("project's instructions")));
}

#[test]
fn a_harness_tool_cap_replaces_the_default() {
    let model = Arc::new(Scripted::new(vec![], true));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness {
            tool_cap: 2,
            ..Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt)
        },
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");
    assert!(!app.tools_exhausted());
    run_one_tool(&mut app, "search_events", "{}");
    assert!(app.tools_exhausted());
    assert_eq!(model.tool_counts(), vec![1, 1, 0]);
}

#[test]
fn a_tool_call_commits_called_then_resulted_then_the_reply() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    // The sequence tui::handle_stream drives for one tool round.
    let _ = app.submit("what happened".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");
    app.append_chunk(Chunk::Reply("found it".to_string()));
    app.end_stream().unwrap();

    let events = app.events();
    assert_eq!(events.len(), 4);
    assert!(events[1].actor() == Actor::Agent);
    assert!(matches!(
        events[1].payload(),
        Payload::ToolCalled { tool, .. } if tool == "search_events"
    ));
    assert!(events[2].actor() == Actor::System);
    assert!(matches!(
        events[2].payload(),
        Payload::ToolResulted { content } if content == "ran"
    ));
    assert!(events[2].causation_id() == Some(events[1].id()));
    // The reply chains off the tool result, not the prompt.
    assert!(events[3].causation_id() == Some(events[2].id()));
    assert_eq!(content(&events[3]), "found it");
}

#[test]
fn a_tool_round_commits_model_called_before_tool_called() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("what happened".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(usage()));
    run_one_tool(&mut app, "search_events", "{}");

    let events = app.events();
    assert_eq!(events.len(), 4);
    assert!(matches!(events[1].payload(), Payload::ModelCalled(..)));
    assert!(events[1].actor() == Actor::System);
    assert!(events[1].causation_id() == Some(events[0].id()));
    assert!(matches!(events[2].payload(), Payload::ToolCalled { .. }));
    assert!(matches!(events[3].payload(), Payload::ToolResulted { .. }));
}

#[test]
fn an_unknown_tool_name_becomes_the_result_content() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "nope", "{}");

    match app.events()[2].payload() {
        Payload::ToolResulted { content } => assert_eq!(content, "no such tool: nope"),
        _ => panic!("expected a tool.resulted event"),
    }
}

/// A tool whose output carries commits of its own, the way
/// `revise_map` records what it judged from the log.
struct Committing(Vec<Payload>);

impl crate::harness::Tool for Committing {
    fn spec(&self) -> crate::harness::ToolSpec {
        crate::harness::ToolSpec {
            name: "search_events",
            description: "a fake that commits what it was given",
            parameters: "{}",
        }
    }

    fn run(
        &self,
        _arguments: &str,
    ) -> Result<crate::harness::ToolOutput, Box<dyn std::error::Error>> {
        Ok(crate::harness::ToolOutput {
            content: "recorded".to_string(),
            commits: self.0.clone(),
        })
    }
}

#[test]
fn a_tool_s_commits_land_between_the_call_and_the_result_caused_by_it() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(
            vec![Arc::new(Committing(vec![
                Payload::MessageReceived {
                    content: "one".to_string(),
                },
                Payload::MessageReceived {
                    content: "two".to_string(),
                },
            ]))],
            MapShape::Prompt,
        ),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");

    let events = app.events();
    assert_eq!(events.len(), 5);
    let called_id = events[1].id();
    assert!(matches!(events[1].payload(), Payload::ToolCalled { .. }));
    assert!(events[2].actor() == Actor::Agent);
    assert_eq!(content(&events[2]), "one");
    assert!(events[2].causation_id() == Some(called_id));
    assert!(events[3].actor() == Actor::Agent);
    assert_eq!(content(&events[3]), "two");
    assert!(events[3].causation_id() == Some(called_id));
    assert!(matches!(
        events[4].payload(),
        Payload::ToolResulted { content } if content == "recorded"
    ));
    assert!(events[4].causation_id() == Some(called_id));
}

#[test]
fn the_tool_call_limit_stops_tools_being_sent_and_then_exhausts() {
    let model = Arc::new(Scripted::new(vec![], true));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    assert!(!app.tools_exhausted());
    for _ in 0..MAX_TOOL_CALLS {
        run_one_tool(&mut app, "search_events", "{}");
    }
    assert!(app.tools_exhausted());

    let counts = model.tool_counts();
    // submit, then one re-ask per finished tool call.
    assert_eq!(counts.len(), MAX_TOOL_CALLS + 1);
    assert!(counts[..MAX_TOOL_CALLS].iter().all(|&n| n == 1));
    // The request after the last allowed call carries no tools.
    assert_eq!(counts[MAX_TOOL_CALLS], 0);
}

#[test]
fn the_request_after_the_last_tool_call_says_the_budget_is_spent() {
    let model = Arc::new(Scripted::new(vec![], true));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    for _ in 0..MAX_TOOL_CALLS {
        run_one_tool(&mut app, "search_events", "{}");
    }

    assert!(model
        .last_request()
        .iter()
        .any(|message| message.contains("tool budget is spent")));
}

#[test]
fn a_model_that_cannot_use_tools_is_sent_none() {
    let model = Arc::new(Scripted::new(vec![vec![]], false));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();

    assert_eq!(model.tool_counts()[0], 0);
}

fn seeded_app(
    events: Vec<Event>,
    tools: Vec<Arc<dyn crate::harness::Tool>>,
) -> (Arc<Scripted>, App) {
    seeded_app_with_shape(events, tools, MapShape::Prompt)
}

fn seeded_app_with_shape(
    events: Vec<Event>,
    tools: Vec<Arc<dyn crate::harness::Tool>>,
    map_shape: MapShape,
) -> (Arc<Scripted>, App) {
    let model = Arc::new(Scripted::new(vec![], true));
    let app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::seeded(events)),
        Arc::new(schemas()),
        Harness::new(tools, map_shape),
        source(SOURCE),
        human(),
    )
    .unwrap();
    (model, app)
}

/// `n` user prompts of ten tokens each, named `0` to `n-1`. The
/// scripted model's window is 1000 tokens, so twenty-five of them
/// overflow the eighth that history may take.
fn filler(n: usize) -> Vec<Event> {
    (0..n)
        .map(|i| Event::message_received(Actor::Human(human()), format!("{i:<40}"), source(SOURCE), None))
        .collect()
}

/// Whether a request carries a filler prompt named `name`.
fn has_filler(sent: &[String], name: &str) -> bool {
    sent.iter()
        .any(|message| message.split_whitespace().next() == Some(name))
}

#[test]
fn a_log_longer_than_the_window_sends_only_its_newest_events() {
    let (model, mut app) = seeded_app(filler(25), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    // The whole log stays in the transcript the TUI renders.
    assert_eq!(app.events().len(), 26);
    let sent = model.last_request();
    assert!(!has_filler(&sent, "0"));
    assert!(has_filler(&sent, "24"));
    assert!(sent.contains(&"now".to_string()));
}

#[test]
fn a_long_tool_loop_never_evicts_the_prompt_it_is_answering() {
    let (model, mut app) = seeded_app(Vec::new(), vec![Arc::new(FakeTool)]);

    let _ = app.submit("the question".to_string()).unwrap();
    // Each round commits four events: thought, reply, call, result.
    // Five hundred tokens of replies are four times what history may
    // take of the scripted model's window.
    for _ in 0..MAX_TOOL_CALLS {
        app.append_chunk(Chunk::Thought("hm".to_string()));
        app.append_chunk(Chunk::Reply("looking ".repeat(50)));
        run_one_tool(&mut app, "search_events", "{}");
    }

    assert!(model.last_request().contains(&"the question".to_string()));
}

#[test]
fn a_map_is_sent_with_its_kinds_ahead_of_the_transcript_and_outside_the_window() {
    let mut events = vec![Event::new(
        Actor::Human(human()),
        source(SOURCE),
        None,
        crate::core::Payload::NodeAdded {
            map: "decisions".to_string(),
            node: crate::core::NodeId::new(),
            kind: "decision".to_string(),
            name: "Rust over Go".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
            seq: 1,
        },
    )];
    events.extend(filler(25));
    let (model, mut app) = seeded_app(events, Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(!has_filler(&sent, "0"));
    assert_decisions_header(&sent[0]);
    assert!(sent[0].contains("- decision \"Rust over Go\""));
}

/// The catalogue line every shape of the decisions map opens with.
fn assert_decisions_header(message: &str) {
    assert!(message.starts_with(
        "The decisions map: what was asked, what was chosen, and why, so a settled question is \
         not reopened. It holds "
    ));
    assert!(message.contains(
        ". Node kinds: `question`, `option` (requires `why`), `evidence`, `decision`. Edge \
         kinds: `answers` (option -> question), `supports` (evidence -> option), `contradicts` \
         (evidence -> option), `resolves` (decision -> question), `supersedes` (decision -> \
         decision), `reopens` (question -> decision).\n"
    ));
}

#[test]
fn an_empty_map_is_still_sent_with_its_kinds() {
    let (model, mut app) = seeded_app(Vec::new(), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    // One message per map, the prompt, the time.
    assert_eq!(sent.len(), 2 + schemas().folded().count());
    assert!(sent[0].contains(
        "Node kinds: `question`, `option` (requires `why`), `evidence`, `decision`."
    ));
    assert!(sent[0].contains("\n(empty:"), "{}", sent[0]);
}

#[test]
fn a_headlines_map_sends_only_its_headline_nodes() {
    let mut events = vec![
        node_added("decision", "Rust over Go"),
        node_added("evidence", "benchmarks"),
    ];
    events.extend(filler(25));
    let (model, mut app) = seeded_app_with_shape(events, Vec::new(), MapShape::Headlines);

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert_decisions_header(&sent[0]);
    assert!(
        sent[0].contains(
            "Its question and decision nodes follow; read_map opens the rest, whole or around one node.\n"
        )
    );
    assert!(sent[0].contains("- decision \"Rust over Go\""));
    assert!(!sent[0].contains("benchmarks"));
}

#[test]
fn a_tool_shape_map_sends_only_its_size() {
    let mut events = vec![
        node_added("decision", "Rust over Go"),
        node_added("evidence", "benchmarks"),
    ];
    events.extend(filler(25));
    let (model, mut app) = seeded_app_with_shape(events, Vec::new(), MapShape::Tool);

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert_decisions_header(&sent[0]);
    assert!(sent[0].contains("It holds 2 nodes and 0 edges, last changed "));
    assert!(sent[0].ends_with("\nread_map shows it."));
    assert!(!sent[0].contains("Rust over Go"));
}

#[test]
fn a_map_header_carries_its_purpose_size_and_last_change() {
    let events = vec![node_added("decision", "Rust over Go")];
    let changed = events[0].created_at();
    let (model, mut app) = seeded_app(events, Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(sent[0].starts_with(&format!(
        "The decisions map: {}. It holds 1 nodes and 0 edges, last changed {changed}. Node kinds:",
        crate::core::testing::decisions().purpose
    )));
}

#[test]
fn an_empty_map_header_says_it_holds_nothing_yet() {
    let (model, mut app) = seeded_app(Vec::new(), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(
        sent[0].contains(". It holds nothing yet. Node kinds:"),
        "{}",
        sent[0]
    );
}

#[test]
fn a_map_that_does_not_fold_fails_at_open() {
    let events = vec![Event::new(
        Actor::Human(human()),
        source(SOURCE),
        None,
        crate::core::Payload::NodeAdded {
            map: "decisions".to_string(),
            node: crate::core::NodeId::new(),
            kind: "goal".to_string(),
            name: "Ship".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
            seq: 1,
        },
    )];
    let err = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::seeded(events)),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .err()
    .unwrap();

    assert!(err.to_string().contains("no node kind \"goal\""));
}

#[test]
fn a_tool_commit_the_transcript_cannot_fold_becomes_the_result_not_a_crash() {
    let dangling = Payload::EdgeAdded {
        map: "decisions".to_string(),
        kind: "supports".to_string(),
        from: crate::core::NodeId::new(),
        to: crate::core::NodeId::new(),
        sources: Vec::new(),
    };
    let (_, mut app) = seeded_app(Vec::new(), vec![Arc::new(Committing(vec![dangling]))]);

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");

    let events = app.events();
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[2].payload(),
        Payload::ToolResulted { content } if content.contains("does not fit its map")
    ));
}

#[test]
fn a_reflect_prompt_replays_in_its_own_turn_and_never_after() {
    let (model, mut app) = seeded_app(Vec::new(), Vec::new());

    let _ = app
        .submit_as(Actor::System, "revise the map".to_string())
        .unwrap();
    let prompt = app.events().last().unwrap();
    assert!(prompt.actor() == Actor::System);
    assert_eq!(content(prompt), "revise the map");
    assert!(model.last_request().contains(&"revise the map".to_string()));
    app.end_stream().unwrap();

    let _ = app.submit("what did we decide?".to_string()).unwrap();

    let sent = model.last_request();
    assert!(!sent.contains(&"revise the map".to_string()));
    assert!(sent.contains(&"what did we decide?".to_string()));
}

#[test]
fn a_log_shorter_than_the_window_sends_all_of_it() {
    let (model, mut app) = seeded_app(filler(3), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    // One message per map, three events, the prompt, the time.
    assert_eq!(model.last_request().len(), 5 + schemas().folded().count());
}

#[test]
fn a_model_called_event_never_reaches_the_next_request() {
    let (model, mut app) = seeded_app(Vec::new(), Vec::new());

    let _ = app.submit("first".to_string()).unwrap();
    app.append_chunk(Chunk::Reply("ok".to_string()));
    app.append_chunk(Chunk::Usage(usage()));
    app.end_stream().unwrap();

    let _ = app.submit("second".to_string()).unwrap();

    let sent = model.last_request();
    // One message per map, "first", "ok", "second", the time - the
    // model.called event between "ok" and "second" is never one of
    // them.
    assert_eq!(sent.len(), 4 + schemas().folded().count());
    assert!(sent.contains(&"first".to_string()));
    assert!(sent.contains(&"ok".to_string()));
    assert!(sent.contains(&"second".to_string()));
}

#[test]
fn set_model_swaps_the_live_model() {
    let scripted: Arc<dyn crate::harness::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = crate::harness::ModelDescriptor {
        provider: crate::harness::Provider::Ollama,
        model: "scripted".to_string(),
        reasoning_efforts: &[],
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    assert_eq!(app.model_name(), "silent");

    app.set_model(&descriptor).unwrap();

    assert_eq!(app.model_name(), "scripted");
}

#[test]
fn set_model_clears_last_usage_so_the_new_model_reads_as_unasked() {
    let scripted: Arc<dyn crate::harness::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = crate::harness::ModelDescriptor {
        provider: crate::harness::Provider::Ollama,
        model: "scripted".to_string(),
        reasoning_efforts: &[],
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    let _ = app.submit("hi".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(usage()));
    app.end_stream().unwrap();
    assert!(app.last_usage().is_some());

    app.set_model(&descriptor).unwrap();

    assert!(app.last_usage().is_none());
}

#[test]
fn set_model_errs_and_leaves_the_model_in_place_while_a_turn_streams() {
    let scripted: Arc<dyn crate::harness::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = crate::harness::ModelDescriptor {
        provider: crate::harness::Provider::Ollama,
        model: "scripted".to_string(),
        reasoning_efforts: &[],
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    assert!(app.is_replying());

    assert!(app.set_model(&descriptor).is_err());
    assert_eq!(app.model_name(), "silent");
}

fn app_on(model: Arc<dyn crate::harness::Model>, catalog: FakeCatalog) -> App {
    App::new(
        model,
        Arc::new(catalog),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap()
}

fn thinking_model() -> Arc<dyn crate::harness::Model> {
    Arc::new(
        Scripted::new(vec![], true).with_reasoning_efforts(crate::harness::ReasoningEffort::ALL),
    )
}

#[test]
fn a_model_with_a_reasoning_control_starts_at_its_default_level() {
    let app = app_on(thinking_model(), FakeCatalog::default());

    assert_eq!(
        app.reasoning_effort(),
        Some(crate::harness::ReasoningEffort::Low)
    );
}

#[test]
fn set_reasoning_effort_refuses_a_level_the_model_cannot_use() {
    let mut app = app_on(Arc::new(Silent), FakeCatalog::default());

    assert!(app
        .set_reasoning_effort(crate::harness::ReasoningEffort::Low)
        .is_err());
    assert_eq!(app.reasoning_effort(), None);
}

#[test]
fn set_reasoning_effort_takes_a_level_the_model_supports() {
    let mut app = app_on(thinking_model(), FakeCatalog::default());

    app.set_reasoning_effort(crate::harness::ReasoningEffort::High)
        .unwrap();

    assert_eq!(
        app.reasoning_effort(),
        Some(crate::harness::ReasoningEffort::High)
    );
}

#[test]
fn switching_to_a_model_without_a_reasoning_control_drops_the_selection() {
    let plain: Arc<dyn crate::harness::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = crate::harness::ModelDescriptor {
        provider: crate::harness::Provider::Ollama,
        model: "plain".to_string(),
        reasoning_efforts: &[],
    };
    let catalog = FakeCatalog::new(vec![descriptor.clone()], vec![(descriptor.clone(), plain)]);
    let mut app = app_on(thinking_model(), catalog);
    app.set_reasoning_effort(crate::harness::ReasoningEffort::High)
        .unwrap();

    app.set_model(&descriptor).unwrap();

    assert_eq!(app.reasoning_effort(), None);
}

#[test]
fn switching_models_keeps_a_selected_level_the_new_model_also_supports() {
    let descriptor = crate::harness::ModelDescriptor {
        provider: crate::harness::Provider::OpenAi,
        model: "next".to_string(),
        reasoning_efforts: crate::harness::ReasoningEffort::ALL,
    };
    let catalog = FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), thinking_model())],
    );
    let mut app = app_on(thinking_model(), catalog);
    app.set_reasoning_effort(crate::harness::ReasoningEffort::High)
        .unwrap();

    app.set_model(&descriptor).unwrap();

    assert_eq!(
        app.reasoning_effort(),
        Some(crate::harness::ReasoningEffort::High)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn available_models_returns_the_catalog_s_listing() {
    let descriptors = vec![
        crate::harness::ModelDescriptor {
            provider: crate::harness::Provider::Ollama,
            model: "gemma".to_string(),
            reasoning_efforts: &[],
        },
        crate::harness::ModelDescriptor {
            provider: crate::harness::Provider::OpenAi,
            model: "gpt".to_string(),
            reasoning_efforts: &[],
        },
    ];
    let catalog = Arc::new(FakeCatalog::new(descriptors.clone(), Vec::new()));
    let app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();

    let listed = app.available_models().await;

    assert_eq!(listed, descriptors);
}

#[test]
fn last_usage_is_the_most_recent_round_trip_not_a_sum() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source(SOURCE),
        human(),
    )
    .unwrap();
    assert!(app.last_usage().is_none());

    let _ = app.submit("first".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(crate::core::Usage {
        input_tokens: 100,
        ..usage()
    }));
    app.end_stream().unwrap();
    assert_eq!(app.last_usage().unwrap().input_tokens, 100);

    let _ = app.submit("second".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(crate::core::Usage {
        input_tokens: 250,
        ..usage()
    }));
    app.end_stream().unwrap();
    assert_eq!(app.last_usage().unwrap().input_tokens, 250);
}
