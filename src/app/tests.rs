use super::*;
use crate::percept::{Actor, Chunk, Payload};
use crate::testing::{content, node_added, usage, FakeCatalog, FakeLog, FakeTool, Scripted};

const SOURCE: &str = "tui";

fn thought(event: &Event) -> &str {
    match event.payload() {
        Payload::ThoughtRecorded { content } => content,
        _ => panic!("expected a thought.recorded event"),
    }
}

struct Silent;

impl percept::Model for Silent {
    fn capabilities(&self) -> percept::ModelCapabilities {
        percept::ModelCapabilities {
            input: &[percept::Modality::Text],
            output: &[percept::Modality::Text],
            tool_use: false,
            context_window: None,
        }
    }

    fn name(&self) -> &str {
        "silent"
    }

    fn reply(&self, _request: &percept::ModelRequest) -> percept::ReplyStream {
        Box::pin(tokio_stream::empty())
    }
}

#[test]
fn streamed_reply_commits_one_event_caused_by_the_prompt() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
    assert!(events[0].actor() == Actor::User);
    assert!(events[1].actor() == Actor::Model);
    assert_eq!(content(&events[1]), "hello");
    assert!(events[1].causation_id() == Some(events[0].id()));
    assert_eq!(events[0].source(), SOURCE);
    assert_eq!(events[1].source(), SOURCE);
}

#[test]
fn a_thought_and_a_reply_commit_as_two_model_events_thought_first() {
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
    assert!(events[1].actor() == Actor::Model);
    assert_eq!(thought(&events[1]), "hmm");
    assert!(events[2].actor() == Actor::Model);
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();
    let _ = app.submit("hi".to_string()).unwrap();
    app.end_stream().unwrap();
    assert_eq!(app.events().len(), 1);
}

#[test]
fn preseeded_log_becomes_the_opening_transcript() {
    let seeded = vec![
        Event::message_received(Actor::User, "hi".to_string(), SOURCE.to_string(), None),
        Event::message_received(Actor::Model, "hello".to_string(), SOURCE.to_string(), None),
    ];
    let log = Arc::new(FakeLog::seeded(seeded));
    let mut app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();
    assert_eq!(app.events().len(), 2);

    let _ = app.submit("next".to_string()).unwrap();
    assert_eq!(app.events().len(), 3);
}

#[test]
fn a_reopened_log_s_last_model_called_seeds_last_usage() {
    let seeded = vec![
        Event::message_received(Actor::User, "hi".to_string(), SOURCE.to_string(), None),
        Event::model_called(usage(), SOURCE.to_string(), None),
    ];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();

    assert_eq!(app.last_usage().unwrap(), &usage());
}

#[test]
fn a_log_with_no_model_called_leaves_last_usage_unset() {
    let seeded = vec![Event::message_received(
        Actor::User,
        "hi".to_string(),
        SOURCE.to_string(),
        None,
    )];
    let log = Arc::new(FakeLog::seeded(seeded));
    let app = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        log,
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
    }
}

#[test]
fn a_tool_call_commits_called_then_resulted_then_the_reply() {
    let mut app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        vec![Arc::new(FakeTool)],
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();

    // The sequence tui::handle_stream drives for one tool round.
    let _ = app.submit("what happened".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");
    app.append_chunk(Chunk::Reply("found it".to_string()));
    app.end_stream().unwrap();

    let events = app.events();
    assert_eq!(events.len(), 4);
    assert!(events[1].actor() == Actor::Model);
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
        vec![Arc::new(FakeTool)],
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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

impl percept::Tool for Committing {
    fn spec(&self) -> percept::ToolSpec {
        percept::ToolSpec {
            name: "search_events",
            description: "a fake that commits what it was given",
            parameters: "{}",
        }
    }

    fn run(&self, _arguments: &str) -> Result<percept::ToolOutput, Box<dyn std::error::Error>> {
        Ok(percept::ToolOutput {
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
        vec![Arc::new(Committing(vec![
            Payload::MessageReceived {
                content: "one".to_string(),
            },
            Payload::MessageReceived {
                content: "two".to_string(),
            },
        ]))],
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();

    let _ = app.submit("go".to_string()).unwrap();
    run_one_tool(&mut app, "search_events", "{}");

    let events = app.events();
    assert_eq!(events.len(), 5);
    let called_id = events[1].id();
    assert!(matches!(events[1].payload(), Payload::ToolCalled { .. }));
    assert!(events[2].actor() == Actor::Model);
    assert_eq!(content(&events[2]), "one");
    assert!(events[2].causation_id() == Some(called_id));
    assert!(events[3].actor() == Actor::Model);
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
        vec![Arc::new(FakeTool)],
        MapShape::Prompt,
        SOURCE.to_string(),
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
fn a_model_that_cannot_use_tools_is_sent_none() {
    let model = Arc::new(Scripted::new(vec![vec![]], false));
    let mut app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        vec![Arc::new(FakeTool)],
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();

    assert_eq!(model.tool_counts()[0], 0);
}

fn seeded_app(events: Vec<Event>, tools: Vec<Arc<dyn percept::Tool>>) -> (Arc<Scripted>, App) {
    seeded_app_with_shape(events, tools, MapShape::Prompt)
}

fn seeded_app_with_shape(
    events: Vec<Event>,
    tools: Vec<Arc<dyn percept::Tool>>,
    map_shape: MapShape,
) -> (Arc<Scripted>, App) {
    let model = Arc::new(Scripted::new(vec![], true));
    let app = App::new(
        model.clone(),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::seeded(events)),
        tools,
        map_shape,
        SOURCE.to_string(),
    )
    .unwrap();
    (model, app)
}

fn filler(n: usize) -> Vec<Event> {
    (0..n)
        .map(|i| Event::message_received(Actor::User, i.to_string(), SOURCE.to_string(), None))
        .collect()
}

#[test]
fn a_log_longer_than_the_window_sends_only_its_newest_events() {
    let (model, mut app) = seeded_app(filler(25), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    // The whole log stays in the transcript the TUI renders.
    assert_eq!(app.events().len(), 26);
    let sent = model.last_request();
    // The time, the decisions map, then the window.
    assert_eq!(sent.len(), CONTEXT_EVENTS + 2);
    assert!(!sent.contains(&"0".to_string()));
    assert!(sent.contains(&"24".to_string()));
    assert!(sent.contains(&"now".to_string()));
}

#[test]
fn a_window_opening_on_a_tool_result_drops_it() {
    let mut events = vec![
        Event::tool_called(
            "search_events".to_string(),
            "{}".to_string(),
            SOURCE.to_string(),
            None,
        ),
        Event::tool_resulted("ran".to_string(), SOURCE.to_string(), None),
    ];
    events.extend(filler(CONTEXT_EVENTS - 2));

    // Submitting pushes the call out of the window, leaving its
    // result as the first event the model would otherwise see.
    let (model, mut app) = seeded_app(events, Vec::new());
    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(!sent.contains(&"<result>".to_string()));
    assert_eq!(sent.len(), CONTEXT_EVENTS + 1);
}

#[test]
fn a_long_tool_loop_never_evicts_the_prompt_it_is_answering() {
    let (model, mut app) = seeded_app(Vec::new(), vec![Arc::new(FakeTool)]);

    let _ = app.submit("the question".to_string()).unwrap();
    // Each round commits four events: thought, reply, call, result.
    for _ in 0..MAX_TOOL_CALLS {
        app.append_chunk(Chunk::Thought("hm".to_string()));
        app.append_chunk(Chunk::Reply("looking".to_string()));
        run_one_tool(&mut app, "search_events", "{}");
    }

    // The turn has outgrown the window on its own.
    assert!(app.events().len() > CONTEXT_EVENTS);
    assert!(model.last_request().contains(&"the question".to_string()));
}

#[test]
fn a_map_is_sent_with_its_kinds_ahead_of_the_transcript_and_outside_the_window() {
    let mut events = vec![Event::new(
        Actor::User,
        SOURCE.to_string(),
        None,
        percept::Payload::NodeAdded {
            map: "decisions".to_string(),
            node: percept::NodeId::new(),
            kind: "decision".to_string(),
            name: "Rust over Go".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
        },
    )];
    events.extend(filler(CONTEXT_EVENTS + 5));
    let (model, mut app) = seeded_app(events, Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert_eq!(sent.len(), CONTEXT_EVENTS + 2);
    assert!(sent[1].starts_with(
        "The decisions map, built from this log. Node kinds: question, option, \
         evidence, decision. Edge kinds: supports, contradicts, resolves.\n"
    ));
    assert!(sent[1].contains("- decision \"Rust over Go\""));
}

#[test]
fn an_empty_map_is_still_sent_with_its_kinds() {
    let (model, mut app) = seeded_app(Vec::new(), Vec::new());

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert_eq!(sent.len(), 3);
    assert!(sent[1].contains("Node kinds: question, option, evidence, decision."));
    assert!(sent[1].contains("\n(empty:"), "{}", sent[1]);
}

#[test]
fn a_headlines_map_sends_only_its_headline_nodes() {
    let mut events = vec![
        node_added("decision", "Rust over Go"),
        node_added("evidence", "benchmarks"),
    ];
    events.extend(filler(CONTEXT_EVENTS + 5));
    let (model, mut app) = seeded_app_with_shape(events, Vec::new(), MapShape::Headlines);

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(sent[1].starts_with(
        "The decisions map, built from this log. Node kinds: question, option, \
         evidence, decision. Edge kinds: supports, contradicts, resolves.\n"
    ));
    assert!(
        sent[1].contains("Its question and decision nodes follow; read_map shows the whole map.\n")
    );
    assert!(sent[1].contains("- decision \"Rust over Go\""));
    assert!(!sent[1].contains("benchmarks"));
}

#[test]
fn a_tool_shape_map_sends_only_its_size() {
    let mut events = vec![
        node_added("decision", "Rust over Go"),
        node_added("evidence", "benchmarks"),
    ];
    events.extend(filler(CONTEXT_EVENTS + 5));
    let (model, mut app) = seeded_app_with_shape(events, Vec::new(), MapShape::Tool);

    let _ = app.submit("now".to_string()).unwrap();

    let sent = model.last_request();
    assert!(sent[1].starts_with(
        "The decisions map, built from this log. Node kinds: question, option, \
         evidence, decision. Edge kinds: supports, contradicts, resolves.\n"
    ));
    assert!(sent[1].contains("It holds 2 nodes and 0 edges. read_map shows it."));
    assert!(!sent[1].contains("Rust over Go"));
}

#[test]
fn a_map_that_does_not_fold_fails_at_open() {
    let events = vec![Event::new(
        Actor::User,
        SOURCE.to_string(),
        None,
        percept::Payload::NodeAdded {
            map: "decisions".to_string(),
            node: percept::NodeId::new(),
            kind: "goal".to_string(),
            name: "Ship".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
        },
    )];
    let err = App::new(
        Arc::new(Silent),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::seeded(events)),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        from: percept::NodeId::new(),
        to: percept::NodeId::new(),
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

    // The time, the decisions map, three events, the prompt.
    assert_eq!(model.last_request().len(), 6);
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
    // The time, the decisions map, "first", "ok", "second" - the
    // model.called event between "ok" and "second" is never one of
    // them.
    assert_eq!(sent.len(), 5);
    assert!(sent.contains(&"first".to_string()));
    assert!(sent.contains(&"ok".to_string()));
    assert!(sent.contains(&"second".to_string()));
}

#[test]
fn set_model_swaps_the_live_model() {
    let scripted: Arc<dyn percept::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = percept::ModelDescriptor {
        provider: percept::Provider::Ollama,
        model: "scripted".to_string(),
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();
    assert_eq!(app.model_name(), "silent");

    app.set_model(&descriptor).unwrap();

    assert_eq!(app.model_name(), "scripted");
}

#[test]
fn set_model_clears_last_usage_so_the_new_model_reads_as_unasked() {
    let scripted: Arc<dyn percept::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = percept::ModelDescriptor {
        provider: percept::Provider::Ollama,
        model: "scripted".to_string(),
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
    let scripted: Arc<dyn percept::Model> = Arc::new(Scripted::new(vec![], true));
    let descriptor = percept::ModelDescriptor {
        provider: percept::Provider::Ollama,
        model: "scripted".to_string(),
    };
    let catalog = Arc::new(FakeCatalog::new(
        vec![descriptor.clone()],
        vec![(descriptor.clone(), scripted)],
    ));
    let mut app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();

    let _ = app.submit("hi".to_string()).unwrap();
    assert!(app.is_replying());

    assert!(app.set_model(&descriptor).is_err());
    assert_eq!(app.model_name(), "silent");
}

#[tokio::test(flavor = "current_thread")]
async fn available_models_returns_the_catalog_s_listing() {
    let descriptors = vec![
        percept::ModelDescriptor {
            provider: percept::Provider::Ollama,
            model: "gemma".to_string(),
        },
        percept::ModelDescriptor {
            provider: percept::Provider::OpenAi,
            model: "gpt".to_string(),
        },
    ];
    let catalog = Arc::new(FakeCatalog::new(descriptors.clone(), Vec::new()));
    let app = App::new(
        Arc::new(Silent),
        catalog,
        Arc::new(FakeLog::default()),
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
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
        Vec::new(),
        MapShape::Prompt,
        SOURCE.to_string(),
    )
    .unwrap();
    assert!(app.last_usage().is_none());

    let _ = app.submit("first".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(percept::Usage {
        input_tokens: 100,
        ..usage()
    }));
    app.end_stream().unwrap();
    assert_eq!(app.last_usage().unwrap().input_tokens, 100);

    let _ = app.submit("second".to_string()).unwrap();
    app.append_chunk(Chunk::Usage(percept::Usage {
        input_tokens: 250,
        ..usage()
    }));
    app.end_stream().unwrap();
    assert_eq!(app.last_usage().unwrap().input_tokens, 250);
}
