use super::*;
use crate::app::{App, MapShape};
use crate::percept::{self, Payload};
use crate::testing::{
    content, node_added_at, source, FakeCatalog, FakeLog, FakeRenderer, FakeTool, Scripted,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The project root `publish` stamps every event with, in tests that
/// don't care what it is.
const ROOT: &str = "/test";

fn args(actor: &str, payload: &str) -> PublishArgs {
    PublishArgs {
        actor: actor.to_string(),
        source: "claude-code".to_string(),
        kind: "message.received".to_string(),
        payload: payload.to_string(),
        causation: None,
    }
}

#[test]
fn a_publish_citing_a_cause_records_it() {
    let log = FakeLog::default();
    publish(args("user", r#"{"content":"hi"}"#), &log, Path::new(ROOT)).unwrap();
    let cause = log.load().unwrap()[0].id();

    let mut reply = args("model", r#"{"content":"hello"}"#);
    reply.causation = Some(cause.as_uuid().to_string());
    publish(reply, &log, Path::new(ROOT)).unwrap();

    assert!(log.load().unwrap()[1].causation_id() == Some(cause));
}

#[test]
fn a_publish_citing_a_cause_the_log_lacks_is_rejected() {
    let log = FakeLog::default();
    let mut orphan = args("model", r#"{"content":"hello"}"#);
    orphan.causation = Some(percept::EventId::new().as_uuid().to_string());
    assert!(publish(orphan, &log, Path::new(ROOT)).is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_valid_publish_appends_one_event_carrying_its_source() {
    let log = FakeLog::default();
    publish(args("user", r#"{"content":"hi"}"#), &log, Path::new(ROOT)).unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source().name, "claude-code");
    assert_eq!(events[0].source().path, Path::new(ROOT));
    assert!(events[0].actor() == percept::Actor::User);
}

#[test]
fn a_payload_field_the_type_does_not_record_is_rejected() {
    let log = FakeLog::default();
    let extra = r#"{"content":"hi","meta":{"thread":42}}"#;
    assert!(publish(args("user", extra), &log, Path::new(ROOT)).is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_rejected_event_appends_nothing() {
    let log = FakeLog::default();
    assert!(publish(args("robot", r#"{"content":"hi"}"#), &log, Path::new(ROOT)).is_err());
    assert!(publish(args("user", "not json"), &log, Path::new(ROOT)).is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn parse_time_parses_an_iso8601_timestamp() {
    let parsed = parse_time("since", "2026-01-01T00:00:00Z").unwrap();
    assert_eq!(parsed.to_string(), "2026-01-01T00:00:00Z");
}

#[test]
fn parse_time_parses_relative_shorthand_as_a_time_in_the_past() {
    let now = Timestamp::now();
    for shorthand in ["1d", "2h", "30m"] {
        let parsed = parse_time("until", shorthand).unwrap();
        assert!(parsed < now, "{shorthand} should parse to before now");
    }
}

#[test]
fn parse_time_rejects_an_unparseable_value_and_names_its_flag() {
    let err = parse_time("until", "3x").err().unwrap();
    assert_eq!(err, "invalid --until value 3x");
}

#[test]
fn an_unknown_type_filter_is_rejected_rather_than_matching_nothing() {
    let args = SearchArgs {
        kind: vec!["message.recieved".to_string()],
        ..Default::default()
    };
    assert!(parse_query(&args).is_err());
}

#[test]
fn every_flag_reaches_the_query_it_builds() {
    let args = SearchArgs {
        source: vec!["tui".to_string(), "cli".to_string()],
        actor: vec!["user".to_string()],
        kind: vec!["tool.called".to_string()],
        contains: vec!["deploy".to_string()],
        size: Some(3),
        since: Some("1d".to_string()),
        ..Default::default()
    };

    let query = parse_query(&args).unwrap();

    assert_eq!(query.sources, vec!["tui", "cli"]);
    assert!(query.actors == vec![percept::Actor::User]);
    assert!(query.kinds == vec![percept::EventKind::ToolCalled]);
    assert_eq!(query.text, vec!["deploy".to_string()]);
    assert_eq!(query.size, Some(3));
    assert!(query.since.is_some() && query.until.is_none());
}

#[test]
fn a_window_that_ends_before_it_starts_is_rejected() {
    let args = SearchArgs {
        since: Some("1h".to_string()),
        until: Some("2h".to_string()),
        ..Default::default()
    };
    assert!(parse_query(&args).is_err());
}

#[test]
fn an_unknown_actor_filter_is_rejected_rather_than_matching_nothing() {
    let args = SearchArgs {
        actor: vec!["User".to_string()],
        ..Default::default()
    };
    assert!(parse_query(&args).is_err());
}

#[test]
fn a_blank_contains_value_is_rejected_at_parse() {
    let ok = Cli::try_parse_from(["percept", "events", "search", "--contains", "deploy"]);
    assert!(ok.is_ok());

    let blank = Cli::try_parse_from(["percept", "events", "search", "--contains", " "]);
    assert!(blank.is_err());
}

#[test]
fn a_zero_preview_is_rejected_at_parse() {
    let zero = Cli::try_parse_from(["percept", "events", "search", "--preview", "0"]);
    assert!(zero.is_err());
    let ok = Cli::try_parse_from(["percept", "events", "search", "--preview", "300"]);
    assert!(ok.is_ok());
}

#[test]
fn a_range_without_an_end_reaches_the_end_of_content() {
    let ok = Cli::try_parse_from(["percept", "events", "show", "abc", "--range", "400:"]);
    assert!(ok.is_ok());
}

#[test]
fn a_range_without_a_start_begins_at_zero() {
    let ok = Cli::try_parse_from(["percept", "events", "show", "abc", "--range", ":50"]);
    assert!(ok.is_ok());
}

#[test]
fn preview_and_full_are_refused_together() {
    let both = Cli::try_parse_from(["percept", "events", "search", "--preview", "9", "--full"]);
    assert!(both.is_err());
}

#[test]
fn a_range_with_no_colon_is_rejected_at_parse() {
    let bad = Cli::try_parse_from(["percept", "events", "show", "abc", "--range", "400"]);
    assert!(bad.is_err());
}

#[test]
fn a_prop_splits_on_the_first_equals_sign() {
    let (key, value) = parse_prop("summary=a=b").unwrap();
    assert_eq!(key, "summary");
    assert_eq!(value, "a=b");
}

#[test]
fn a_prop_with_no_equals_sign_is_rejected() {
    assert!(parse_prop("summary").is_err());
}

#[test]
fn a_node_ref_splits_on_the_first_colon() {
    let node = parse_node_ref("option:Rust:the language").unwrap();
    assert_eq!(node.kind, "option");
    assert_eq!(node.name, "Rust:the language");
}

#[test]
fn a_node_ref_with_no_colon_is_rejected() {
    assert!(parse_node_ref("option").is_err());
}

#[test]
fn a_node_ref_with_a_blank_side_is_rejected() {
    assert!(parse_node_ref(":Rust").is_err());
    assert!(parse_node_ref("option: ").is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn ask_runs_one_tool_round_and_commits_the_final_reply() {
    let model = Scripted::new(
        vec![
            vec![percept::Chunk::ToolCall {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            }],
            vec![percept::Chunk::Reply("found it".to_string())],
        ],
        true,
    );
    let log = Arc::new(FakeLog::default());
    let tools: Vec<Arc<dyn percept::Tool>> = vec![Arc::new(FakeTool)];
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        tools,
        Arc::new(FakeRenderer::default()),
        MapShape::Prompt,
        source("cli"),
    )
    .unwrap();

    run_turn(Box::new(app), Actor::User, "what happened".to_string())
        .await
        .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].source().name, "cli");
    assert!(matches!(
        events[1].payload(),
        Payload::ToolCalled { tool, .. } if tool == "search_events"
    ));
    assert!(matches!(
        events[2].payload(),
        Payload::ToolResulted { content } if content == "ran"
    ));
    assert_eq!(content(&events[3]), "found it");
}

#[tokio::test(flavor = "current_thread")]
async fn a_stream_error_ends_the_turn_but_still_commits_partial_text() {
    let log = Arc::new(FakeLog::default());
    // A reply that breaks mid-stream, after saying something.
    let model = Scripted::failing(
        vec![vec![
            Ok(percept::Chunk::Reply("partial".to_string())),
            Err("connection dropped".into()),
        ]],
        false,
    );
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Vec::new(),
        Arc::new(FakeRenderer::default()),
        MapShape::Prompt,
        source("cli"),
    )
    .unwrap();

    let result = run_turn(Box::new(app), Actor::User, "hi".to_string()).await;

    assert!(result.is_err());
    let events = log.load().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(content(&events[1]), "partial");
}

#[test]
fn maps_show_kind_is_repeatable() {
    let cli = Cli::try_parse_from([
        "percept",
        "maps",
        "show",
        "decisions",
        "--kind",
        "question",
        "--kind",
        "decision",
    ])
    .unwrap();
    match cli.command {
        Some(Command::Maps {
            command: MapsCommand::Show(args),
        }) => assert_eq!(args.kind, ["question", "decision"]),
        _ => panic!("expected maps show"),
    }
}

#[test]
fn all_projects_scopes_to_every_project_while_the_default_scopes_to_root() {
    assert_eq!(
        scope(false, Path::new(ROOT)),
        percept::Scope::Project(PathBuf::from(ROOT))
    );
    assert_eq!(scope(true, Path::new(ROOT)), percept::Scope::All);
}

#[test]
fn maps_show_sees_only_this_project_s_nodes_unless_all_projects_is_set() {
    let log = FakeLog::seeded(vec![
        node_added_at(ROOT, "option", "Rust"),
        node_added_at("/elsewhere", "option", "Go"),
    ]);

    let here = store::fold_map(&log, "decisions", &scope(false, Path::new(ROOT))).unwrap();
    let everywhere = store::fold_map(&log, "decisions", &scope(true, Path::new(ROOT))).unwrap();

    assert_eq!(here.nodes().len(), 1);
    assert!(here.find("option", "Rust").is_some());
    assert_eq!(everywhere.nodes().len(), 2);
}

#[test]
fn depth_is_refused_without_around() {
    let alone = Cli::try_parse_from(["percept", "maps", "show", "decisions", "--depth", "2"]);
    assert!(alone.is_err());
    let with = Cli::try_parse_from([
        "percept",
        "maps",
        "show",
        "decisions",
        "--around",
        "question:Why?",
        "--depth",
        "2",
    ]);
    assert!(with.is_ok());
}

#[test]
fn every_write_verb_refuses_the_code_map() {
    let log = FakeLog::default();
    let renderer = FakeRenderer::default();
    let target = || MapArgs {
        map: "code".to_string(),
        source: Vec::new(),
    };

    let cli_source = source("cli");
    let add_node = maps_add_node(
        AddNodeArgs {
            target: target(),
            kind: "file".to_string(),
            name: "src/main.rs".to_string(),
            prop: Vec::new(),
        },
        &log,
        &cli_source,
        &renderer,
    );
    let remove_node = maps_remove_node(
        RemoveNodeArgs {
            target: target(),
            kind: "file".to_string(),
            name: "src/main.rs".to_string(),
            reason: "gone".to_string(),
        },
        &log,
        &cli_source,
        &renderer,
    );
    let edge_args = || EdgeArgs {
        target: target(),
        kind: "imports".to_string(),
        from: NodeRef {
            kind: "file".to_string(),
            name: "src/main.rs".to_string(),
        },
        to: NodeRef {
            kind: "file".to_string(),
            name: "src/app/mod.rs".to_string(),
        },
    };
    let add_edge = maps_add_edge(edge_args(), &log, &cli_source, &renderer);
    let remove_edge = maps_remove_edge(edge_args(), &log, &cli_source, &renderer);

    for result in [add_node, remove_node, add_edge, remove_edge] {
        let err = result.err().unwrap();
        assert!(err.to_string().starts_with("\"code\" is derived"), "{err}");
    }
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn maps_add_node_renders_the_map_it_changed_once() {
    let log = FakeLog::default();
    let renderer = FakeRenderer::default();

    maps_add_node(
        AddNodeArgs {
            target: MapArgs {
                map: "decisions".to_string(),
                source: Vec::new(),
            },
            kind: "decision".to_string(),
            name: "Rust over Go".to_string(),
            prop: Vec::new(),
        },
        &log,
        &source("cli"),
        &renderer,
    )
    .unwrap();

    assert_eq!(renderer.rendered(), vec!["decisions".to_string()]);
}
