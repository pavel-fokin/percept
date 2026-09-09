use super::*;
use crate::app::{App, Harness, MapShape};
use crate::core::testing::{content, schemas, source, FakeLog, Fixture, ROOT};
use crate::core::Payload;
use crate::harness::testing::{FakeCatalog, FakeTool, Scripted};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn args(actor: &str, payload: &str) -> PublishArgs {
    PublishArgs {
        actor: actor.to_string(),
        source: "claude-code".to_string(),
        kind: "message.received".to_string(),
        payload: payload.to_string(),
        causation: None,
    }
}

fn file_seen_args(payload: &str) -> PublishArgs {
    PublishArgs {
        actor: "model".to_string(),
        source: "claude-code".to_string(),
        kind: "file.seen".to_string(),
        payload: payload.to_string(),
        causation: None,
    }
}

/// The checkout most tests never read from - `publish` only opens it
/// for a `file.seen` payload with no `excerpt`.
fn no_checkout() -> &'static Path {
    Path::new(ROOT)
}

#[test]
fn a_publish_citing_a_cause_records_it() {
    let log = FakeLog::default();
    publish(
        args("user", r#"{"content":"hi"}"#),
        &log,
        Path::new(ROOT),
        no_checkout(),
    )
    .unwrap();
    let cause = log.load().unwrap()[0].id();

    let mut reply = args("model", r#"{"content":"hello"}"#);
    reply.causation = Some(cause.as_uuid().to_string());
    publish(reply, &log, Path::new(ROOT), no_checkout()).unwrap();

    assert!(log.load().unwrap()[1].causation_id() == Some(cause));
}

#[test]
fn a_publish_citing_a_cause_the_log_lacks_is_rejected() {
    let log = FakeLog::default();
    let mut orphan = args("model", r#"{"content":"hello"}"#);
    orphan.causation = Some(crate::core::EventId::new().as_uuid().to_string());
    assert!(publish(orphan, &log, Path::new(ROOT), no_checkout()).is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_valid_publish_appends_one_event_carrying_its_source() {
    let log = FakeLog::default();
    publish(
        args("user", r#"{"content":"hi"}"#),
        &log,
        Path::new(ROOT),
        no_checkout(),
    )
    .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].source().name, "claude-code");
    assert_eq!(events[0].source().path, Path::new(ROOT));
    assert!(events[0].actor() == crate::core::Actor::User);
}

#[test]
fn a_payload_field_the_type_does_not_record_is_rejected() {
    let log = FakeLog::default();
    let extra = r#"{"content":"hi","meta":{"thread":42}}"#;
    assert!(publish(args("user", extra), &log, Path::new(ROOT), no_checkout()).is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_rejected_event_appends_nothing() {
    let log = FakeLog::default();
    assert!(publish(
        args("robot", r#"{"content":"hi"}"#),
        &log,
        Path::new(ROOT),
        no_checkout()
    )
    .is_err());
    assert!(publish(
        args("user", "not json"),
        &log,
        Path::new(ROOT),
        no_checkout()
    )
    .is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_file_seen_publish_reads_the_range_from_the_checkout() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "line one\nline two\nline three\nline four\n");
    let log = FakeLog::default();
    let payload = r#"{"path":"src/lib.rs","lines":"2-3"}"#;
    publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap();

    let events = log.load().unwrap();
    match events[0].payload() {
        Payload::FileSeen {
            path,
            lines,
            excerpt,
        } => {
            assert_eq!(path.to_str().unwrap(), "src/lib.rs");
            assert_eq!(*lines, Some((2, 3)));
            assert_eq!(excerpt, "line two\nline three");
        }
        _ => panic!("expected FileSeen"),
    }
}

#[test]
fn a_file_seen_publish_with_no_lines_reads_the_whole_file() {
    let fixture = Fixture::new();
    fixture.write("README.md", "hello\nworld\n");
    let log = FakeLog::default();
    let payload = r#"{"path":"README.md"}"#;
    publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap();

    match log.load().unwrap()[0].payload() {
        Payload::FileSeen { lines, excerpt, .. } => {
            assert_eq!(*lines, None);
            assert_eq!(excerpt, "hello\nworld\n");
        }
        _ => panic!("expected FileSeen"),
    }
}

#[test]
fn a_file_seen_publish_stores_an_absolute_path_relative() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "one\n");
    let log = FakeLog::default();
    let absolute = fixture.path().join("src/lib.rs");
    let payload = format!(r#"{{"path":"{}"}}"#, absolute.to_str().unwrap());
    publish(
        file_seen_args(&payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap();

    match log.load().unwrap()[0].payload() {
        Payload::FileSeen { path, .. } => assert_eq!(path.to_str().unwrap(), "src/lib.rs"),
        _ => panic!("expected FileSeen"),
    }
}

#[test]
fn a_file_seen_publish_with_an_excerpt_stores_it_as_given() {
    let fixture = Fixture::new();
    let log = FakeLog::default();
    // No file on disk at all - an `excerpt` in the payload means the
    // tree is never read.
    let payload = r#"{"path":"src/missing.rs","excerpt":"whatever the caller said"}"#;
    publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap();

    match log.load().unwrap()[0].payload() {
        Payload::FileSeen { excerpt, .. } => {
            assert_eq!(excerpt, "whatever the caller said");
        }
        _ => panic!("expected FileSeen"),
    }
}

#[test]
fn a_file_seen_publish_refuses_a_path_outside_the_checkout() {
    let fixture = Fixture::new();
    let log = FakeLog::default();
    let payload = r#"{"path":"../../etc/passwd"}"#;
    assert!(publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path()
    )
    .is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_file_seen_publish_refuses_a_binary_file() {
    let fixture = Fixture::new();
    let full = fixture.path().join("bin");
    std::fs::write(&full, [0u8, 1, 2, 0, 3]).unwrap();
    let log = FakeLog::default();
    let payload = r#"{"path":"bin"}"#;
    let err = publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("binary"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_file_seen_publish_refuses_a_reversed_range() {
    let fixture = Fixture::new();
    fixture.write("f.txt", "a\nb\nc\n");
    let log = FakeLog::default();
    let payload = r#"{"path":"f.txt","lines":"3-1"}"#;
    assert!(publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path()
    )
    .is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_file_seen_publish_refuses_a_zero_line() {
    let fixture = Fixture::new();
    fixture.write("f.txt", "a\nb\nc\n");
    let log = FakeLog::default();
    let payload = r#"{"path":"f.txt","lines":"0-1"}"#;
    assert!(publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path()
    )
    .is_err());
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_file_seen_publish_refuses_a_range_past_the_end() {
    let fixture = Fixture::new();
    fixture.write("f.txt", "a\nb\nc\n");
    let log = FakeLog::default();
    let payload = r#"{"path":"f.txt","lines":"1-9000"}"#;
    let err = publish(
        file_seen_args(payload),
        &log,
        Path::new(ROOT),
        fixture.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("past"), "{err}");
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
    assert!(query.actors == vec![crate::core::Actor::User]);
    assert!(query.kinds == vec![crate::core::EventKind::ToolCalled]);
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

fn map_with_a_decision() -> Map {
    let mut map = Map::empty(crate::core::testing::decisions());
    map.apply(
        Mutation::AddNode {
            kind: "decision".to_string(),
            name: "Rust over Go".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
        },
        Actor::User,
    )
    .unwrap();
    map
}

#[test]
fn a_node_ref_splits_on_the_first_colon() {
    let node = resolve_ref(&map_with_a_decision(), "decision:Rust over Go").unwrap();
    assert_eq!(node.kind, "decision");
    assert_eq!(node.name, "Rust over Go");
}

#[test]
fn a_node_ref_resolves_by_its_short_id_too() {
    let node = resolve_ref(&map_with_a_decision(), "d1").unwrap();
    assert_eq!(node.kind, "decision");
    assert_eq!(node.name, "Rust over Go");
}

#[test]
fn an_unknown_node_ref_is_rejected() {
    assert!(resolve_ref(&map_with_a_decision(), "decision:Go alone").is_err());
    assert!(resolve_ref(&map_with_a_decision(), "d9").is_err());
}

#[tokio::test(flavor = "current_thread")]
async fn ask_runs_one_tool_round_and_commits_the_final_reply() {
    let model = Scripted::new(
        vec![
            vec![crate::harness::Chunk::ToolCall {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            }],
            vec![crate::harness::Chunk::Reply("found it".to_string())],
        ],
        true,
    );
    let log = Arc::new(FakeLog::default());
    let tools: Vec<Arc<dyn crate::harness::Tool>> = vec![Arc::new(FakeTool)];
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(tools, MapShape::Prompt),
        source("cli"),
    )
    .unwrap();

    run_turn(
        Box::new(app),
        Actor::User,
        "what happened".to_string(),
        false,
    )
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
            Ok(crate::harness::Chunk::Reply("partial".to_string())),
            Err("connection dropped".into()),
        ]],
        false,
    );
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source("cli"),
    )
    .unwrap();

    let result = run_turn(Box::new(app), Actor::User, "hi".to_string(), false).await;

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
fn maps_show_format_defaults_to_json() {
    assert_eq!(
        parse_show(["percept", "maps", "show", "decisions"]).format,
        Format::Json
    );
}

#[test]
fn maps_show_format_md_and_its_markdown_alias_both_parse_to_md() {
    assert_eq!(
        parse_show(["percept", "maps", "show", "decisions", "--format", "md"]).format,
        Format::Md
    );
    assert_eq!(
        parse_show([
            "percept",
            "maps",
            "show",
            "decisions",
            "--format",
            "markdown"
        ])
        .format,
        Format::Md
    );
}

#[test]
fn maps_show_rejects_an_unknown_format() {
    assert!(
        Cli::try_parse_from(["percept", "maps", "show", "decisions", "--format", "yaml"]).is_err()
    );
}

fn parse_show<const N: usize>(argv: [&str; N]) -> ShowMapArgs {
    match Cli::try_parse_from(argv).unwrap().command {
        Some(Command::Maps {
            command: MapsCommand::Show(args),
        }) => args,
        _ => panic!("expected maps show"),
    }
}

#[test]
fn all_projects_scopes_to_every_project_while_the_default_scopes_to_root() {
    assert_eq!(
        scope(false, Path::new(ROOT)),
        crate::core::Scope::Project(PathBuf::from(ROOT))
    );
    assert_eq!(scope(true, Path::new(ROOT)), crate::core::Scope::All);
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
fn since_on_maps_show_parses_like_events_search() {
    let cli =
        Cli::try_parse_from(["percept", "maps", "show", "decisions", "--since", "1d"]).unwrap();
    match cli.command {
        Some(Command::Maps {
            command: MapsCommand::Show(args),
        }) => assert!(args.since.is_some()),
        _ => panic!("expected maps show"),
    }
    assert!(
        Cli::try_parse_from(["percept", "maps", "show", "decisions", "--since", "soon"]).is_err()
    );
}

#[test]
fn every_write_verb_fails_on_a_map_name_no_schema_declares() {
    let log = FakeLog::default();
    let target = || MapArgs {
        map: "code".to_string(),
        source: Vec::new(),
        actor: Actor::User,
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
        &schemas(),
        &cli_source,
    );
    let remove_node = maps_remove_node(
        RemoveNodeArgs {
            target: target(),
            node: "file:src/main.rs".to_string(),
            reason: "gone".to_string(),
        },
        &log,
        &schemas(),
        &cli_source,
    );
    let edge_args = || EdgeArgs {
        target: target(),
        kind: "imports".to_string(),
        from: "file:src/main.rs".to_string(),
        to: "file:src/app/mod.rs".to_string(),
    };
    let add_edge = maps_add_edge(edge_args(), &log, &schemas(), &cli_source);
    let remove_edge = maps_remove_edge(edge_args(), &log, &schemas(), &cli_source);

    for result in [add_node, remove_node, add_edge, remove_edge] {
        let err = result.err().unwrap();
        assert!(err.to_string().starts_with("no map named \"code\""), "{err}");
    }
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_map_write_commits_as_the_actor_given_and_defaults_to_user() {
    let log = FakeLog::default();
    let cli = Cli::try_parse_from([
        "percept",
        "maps",
        "add-node",
        "decisions",
        "--actor",
        "model",
        "--kind",
        "question",
        "--name",
        "Which?",
    ])
    .unwrap();
    let Some(Command::Maps {
        command: MapsCommand::AddNode(args),
    }) = cli.command
    else {
        panic!("expected maps add-node")
    };
    assert!(args.target.actor == Actor::Model);

    maps_add_node(args, &log, &schemas(), &source("cli")).unwrap();

    let events = log.load().unwrap();
    assert!(events[0].actor() == Actor::Model);
    let default = Cli::try_parse_from([
        "percept",
        "maps",
        "add-node",
        "decisions",
        "--kind",
        "question",
        "--name",
        "Which?",
    ])
    .unwrap();
    let Some(Command::Maps {
        command: MapsCommand::AddNode(args),
    }) = default.command
    else {
        panic!("expected maps add-node")
    };
    assert!(args.target.actor == Actor::User);
}
