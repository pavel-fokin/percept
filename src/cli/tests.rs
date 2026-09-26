use super::*;
use crate::core::testing::{
    human, node_added, node_added_by, node_id, schemas, source, FakeLog, FakeSchemas, Fixture, HOME,
    ROOT,
};
use crate::core::{Actor, EdgeKind, NodeKind, Payload};
use std::path::Path;

/// A minimal schema for a test that needs a map of its own, distinct
/// from `debates`/`chores` - a `concept`, prefixed `c`, and a `relates`
/// edge between two of them.
fn concepts_schema() -> Schema {
    Schema::new(
        "concepts",
        "what a test needs from a concepts map",
        vec![NodeKind::new("concept", vec![("definition".to_string(), Vec::new())]).unwrap()],
        vec![EdgeKind::new("relates", vec!["concept".to_string()], vec!["concept".to_string()]).unwrap()],
    )
    .unwrap()
}

/// A minimal global schema, prefixed `p` - distinct from `concepts_schema`
/// so an edge across the two is unambiguous, and global so a test can
/// tell its map lands at `HOME`.
fn projects_schema() -> Schema {
    Schema::new(
        "projects",
        "what a test needs from a global projects map",
        vec![NodeKind::new("project", vec![("root".to_string(), Vec::new())]).unwrap()],
        Vec::new(),
    )
    .unwrap()
}

/// `concepts_schema` and `projects_schema` together, `projects` global.
fn concepts_and_projects() -> FakeSchemas {
    FakeSchemas::with_global(vec![concepts_schema(), projects_schema()], &["projects"])
}

/// `concepts_and_projects`, folded in the order the real loader always
/// gives - a global schema first - for a test of `show`'s own listing
/// order, which `FakeSchemas` otherwise keeps as constructed.
fn projects_and_concepts() -> FakeSchemas {
    FakeSchemas::with_global(vec![projects_schema(), concepts_schema()], &["projects"])
}

/// A `WriteArgs` a test doesn't otherwise care about: `human`, no
/// sources, no explicit causation.
fn write_args() -> WriteArgs {
    WriteArgs::default()
}

/// `chores` alone, for a document test that resolves a bare short id:
/// the combined `schemas()` fixture gives `debates`'s `claim` and
/// `chores`'s own `chore` the same default prefix, `c`, since it never
/// runs through the `check_across` a real project's schemas do - this
/// sidesteps that clash rather than relying on which of the two a
/// lookup happens to find first.
fn chores_only() -> FakeSchemas {
    FakeSchemas::new(vec![crate::core::testing::chores()])
}

/// The checkout most tests never read from - only a `cites` line does.
fn no_checkout() -> &'static Path {
    Path::new(ROOT)
}

fn map_created(name: &str) -> Event {
    Event::map_created(
        crate::core::testing::map_id(name),
        name.to_string(),
        source("test"),
    )
}

/// The refusal `result` carries - `Payload` is not `Debug`, so the
/// error comes out as its own text rather than through `unwrap_err`.
fn err_of(result: Result<Payload, Box<dyn std::error::Error>>) -> String {
    match result {
        Err(err) => err.to_string(),
        Ok(_) => panic!("expected a refusal"),
    }
}

/// A `Workspace` over `fixture`'s root - what a `cites` line resolves
/// its path inside.
fn workspace_at(fixture: &Fixture) -> workspace::Workspace {
    workspace::Workspace::new(fixture.path()).unwrap()
}

#[test]
fn a_citation_reads_its_range_from_the_checkout() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "line one\nline two\nline three\nline four\n");

    let payload = build_file_cited(&workspace_at(&fixture), "src/lib.rs", Some((2, 3)), None).unwrap();

    match payload {
        Payload::FileCited { path, lines, excerpt } => {
            assert_eq!(path.to_str().unwrap(), "src/lib.rs");
            assert_eq!(lines, Some((2, 3)));
            assert_eq!(excerpt, "line two\nline three");
        }
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn a_citation_with_no_lines_reads_the_whole_file() {
    let fixture = Fixture::new();
    fixture.write("README.md", "hello\nworld\n");

    let payload = build_file_cited(&workspace_at(&fixture), "README.md", None, None).unwrap();

    match payload {
        Payload::FileCited { lines, excerpt, .. } => {
            assert_eq!(lines, None);
            assert_eq!(excerpt, "hello\nworld\n");
        }
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn a_citation_stores_an_absolute_path_relative() {
    let fixture = Fixture::new();
    fixture.write("src/lib.rs", "one\n");
    let absolute = fixture.path().join("src/lib.rs");

    let payload =
        build_file_cited(&workspace_at(&fixture), absolute.to_str().unwrap(), None, None).unwrap();

    match payload {
        Payload::FileCited { path, .. } => assert_eq!(path.to_str().unwrap(), "src/lib.rs"),
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn a_citation_given_an_excerpt_never_reads_the_tree() {
    let fixture = Fixture::new();

    // No file on disk at all.
    let payload = build_file_cited(
        &workspace_at(&fixture),
        "src/missing.rs",
        None,
        Some("whatever the caller said".to_string()),
    )
    .unwrap();

    match payload {
        Payload::FileCited { excerpt, .. } => assert_eq!(excerpt, "whatever the caller said"),
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn a_citation_refuses_a_path_outside_the_checkout() {
    let fixture = Fixture::new();

    assert!(build_file_cited(&workspace_at(&fixture), "../../etc/passwd", None, None).is_err());
}

#[test]
fn a_citation_refuses_a_binary_file() {
    let fixture = Fixture::new();
    std::fs::write(fixture.path().join("bin"), [0u8, 1, 2, 0, 3]).unwrap();

    let err = err_of(build_file_cited(&workspace_at(&fixture), "bin", None, None));

    assert!(err.contains("binary"), "{err}");
}

#[test]
fn a_citation_refuses_a_blank_excerpt() {
    let fixture = Fixture::new();

    let err = err_of(build_file_cited(
        &workspace_at(&fixture),
        "src/missing.rs",
        None,
        Some("   \n  ".to_string()),
    ));

    assert!(err.contains("blank"), "{err}");
}

#[test]
fn a_citation_refuses_a_range_past_the_end() {
    let fixture = Fixture::new();
    fixture.write("f.txt", "a\nb\nc\n");

    let err = err_of(build_file_cited(&workspace_at(&fixture), "f.txt", Some((1, 9000)), None));

    assert!(err.contains("past"), "{err}");
}

#[test]
fn an_unknown_type_filter_is_rejected_rather_than_matching_nothing() {
    let args = SearchArgs {
        kind: vec!["message.recieved".to_string()],
        ..Default::default()
    };
    assert!(parse_query(&args, crate::core::testing::human(), None).is_err());
}

#[test]
fn every_flag_reaches_the_query_it_builds() {
    let args = SearchArgs {
        source: vec!["tui".to_string(), "cli".to_string()],
        actor: vec!["user".to_string()],
        kind: vec!["tool.called".to_string()],
        text: vec!["deploy".to_string()],
        size: Some(3),
        since: Some("1d".to_string()),
        ..Default::default()
    };

    let me = crate::core::testing::human();
    let query = parse_query(&args, me, None).unwrap();

    assert_eq!(query.sources, vec!["tui", "cli"]);
    assert!(query.actors == vec![crate::core::Actor::Human(me)]);
    assert!(query.kinds == vec![crate::core::EventKind::ToolCalled]);
    assert_eq!(query.text, vec!["deploy".to_string()]);
    assert_eq!(query.size, Some(3));
    assert!(query.since.is_some() && query.until.is_none());
}

#[test]
fn a_search_inside_a_project_keeps_to_its_events() {
    let project = Path::new("/home/code/app");

    let query = parse_query(&SearchArgs::default(), crate::core::testing::human(), Some(project)).unwrap();

    assert_eq!(query.roots, vec![project.to_path_buf()]);
}

#[test]
fn a_search_at_the_home_level_spans_every_project() {
    let query = parse_query(&SearchArgs::default(), crate::core::testing::human(), None).unwrap();

    assert!(query.roots.is_empty());
}

#[test]
fn a_window_that_ends_before_it_starts_is_rejected() {
    let args = SearchArgs {
        since: Some("1h".to_string()),
        until: Some("2h".to_string()),
        ..Default::default()
    };
    assert!(parse_query(&args, crate::core::testing::human(), None).is_err());
}

#[test]
fn an_unknown_actor_filter_is_rejected_rather_than_matching_nothing() {
    let args = SearchArgs {
        actor: vec!["User".to_string()],
        ..Default::default()
    };
    assert!(parse_query(&args, crate::core::testing::human(), None).is_err());
}

#[test]
fn a_blank_search_text_is_rejected_at_parse() {
    let ok = Cli::try_parse_from(["percept", "search", "deploy"]);
    assert!(ok.is_ok());

    let blank = Cli::try_parse_from(["percept", "search", " "]);
    assert!(blank.is_err());
}

#[test]
fn a_zero_preview_is_rejected_at_parse() {
    let zero = Cli::try_parse_from(["percept", "search", "--preview", "0"]);
    assert!(zero.is_err());
    let ok = Cli::try_parse_from(["percept", "search", "--preview", "300"]);
    assert!(ok.is_ok());
}

#[test]
fn a_range_without_an_end_reaches_the_end_of_content() {
    let ok = Cli::try_parse_from(["percept", "show", "abc", "--range", "400:"]);
    assert!(ok.is_ok());
}

#[test]
fn a_range_without_a_start_begins_at_zero() {
    let ok = Cli::try_parse_from(["percept", "show", "abc", "--range", ":50"]);
    assert!(ok.is_ok());
}

#[test]
fn preview_and_full_are_refused_together() {
    let both = Cli::try_parse_from(["percept", "search", "--preview", "9", "--full"]);
    assert!(both.is_err());
}

#[test]
fn a_range_with_no_colon_is_rejected_at_parse() {
    let bad = Cli::try_parse_from(["percept", "show", "abc", "--range", "400"]);
    assert!(bad.is_err());
}

fn map_with_a_verdict() -> Map {
    let mut map = Map::empty(crate::core::testing::map_id("debates"), crate::core::testing::debates());
    map.apply(
        Mutation::AddNode {
            kind: "verdict".to_string(),
            name: "Rust over Go".to_string(),
            properties: Default::default(),
            sources: Vec::new(),
        },
        Actor::Human(human()),
    )
    .unwrap();
    map
}

#[test]
fn a_node_ref_splits_on_the_first_colon() {
    let node = resolve_ref(&map_with_a_verdict(), "verdict:Rust over Go").unwrap();
    assert_eq!(node.kind, "verdict");
    assert_eq!(node.name, "Rust over Go");
}

#[test]
fn a_node_ref_resolves_by_its_short_id_too() {
    let node = resolve_ref(&map_with_a_verdict(), "v1").unwrap();
    assert_eq!(node.kind, "verdict");
    assert_eq!(node.name, "Rust over Go");
}

#[test]
fn an_unknown_node_ref_is_rejected() {
    assert!(resolve_ref(&map_with_a_verdict(), "verdict:Go alone").is_err());
    assert!(resolve_ref(&map_with_a_verdict(), "v9").is_err());
}

#[test]
fn show_kind_is_repeatable() {
    let args = parse_show(["percept", "show", "debates", "--kind", "topic", "--kind", "verdict"]);
    assert_eq!(args.kind, ["topic", "verdict"]);
}

#[test]
fn show_json_defaults_to_false() {
    assert!(!parse_show(["percept", "show", "debates"]).json);
}

#[test]
fn show_json_flag_sets_it() {
    assert!(parse_show(["percept", "show", "debates", "--json"]).json);
}

fn parse_show<const N: usize>(argv: [&str; N]) -> ShowArgs {
    match Cli::try_parse_from(argv).unwrap().command {
        Some(Command::Show(args)) => args,
        _ => panic!("expected show"),
    }
}

#[test]
fn since_on_show_parses_like_events_search() {
    let args = parse_show(["percept", "show", "debates", "--since", "1d"]);
    assert!(args.since.is_some());
    assert!(Cli::try_parse_from(["percept", "show", "debates", "--since", "soon"]).is_err());
}

#[test]
fn depth_on_show_defaults_to_none_and_parses_when_given() {
    assert!(parse_show(["percept", "show", "d41"]).depth.is_none());
    assert_eq!(parse_show(["percept", "show", "d41", "--depth", "2"]).depth, Some(2));
}

#[test]
fn bare_show_lists_a_global_map_before_a_project_map_with_its_level() {
    let schemas = projects_and_concepts();

    let lines = map_summary_lines(&schemas, &[], Path::new(ROOT), false).unwrap();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("projects"), "{}", lines[0]);
    assert!(lines[0].contains("(~)"), "{}", lines[0]);
    assert!(lines[1].starts_with("concepts"), "{}", lines[1]);
    assert!(lines[1].contains("(project)"), "{}", lines[1]);
}

/// `ShowArgs` naming `arg`, every other flag at its default.
fn show_args(arg: &str) -> ShowArgs {
    ShowArgs { arg: Some(arg.to_string()), ..ShowArgs::default() }
}

#[test]
fn show_of_a_node_prints_it_and_its_neighbour() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    for tokens in [["concept", "Snapshot"], ["concept", "Undo"]] {
        add(add_args(&tokens), &log, &schemas, &source("cli"), no_checkout(), human(), None).unwrap();
    }
    add(
        add_args(&["relates", "c1", "c2"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    // `show` resolves "c1" as a node - the write above already proved
    // `add`'s own resolution works, so a successful call here proves
    // `show` finds the same map with no name of its own.
    show(show_args("c1"), &log, &schemas, Path::new(ROOT)).unwrap();

    let map = mapstore::fold_map(&log, &schemas, "concepts", Path::new(ROOT)).unwrap();
    let around = NodeRef { kind: "concept".to_string(), name: "Snapshot".to_string() };
    let selection = crate::core::Selection { around: Some((&around, 1)), ..crate::core::Selection::default() };
    let fragment = map.select(&selection).unwrap();
    assert!(fragment.map().find("concept", "Snapshot").is_some());
    assert!(fragment.map().find("concept", "Undo").is_some());
}

#[test]
fn show_of_a_uuid_reads_the_event() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    let seed = Event::message_received(Actor::Human(human()), "hi".to_string(), source("cli"), None);
    log.append(&seed).unwrap();
    let id = seed.id().as_uuid().to_string();

    show(show_args(&id), &log, &schemas, Path::new(ROOT)).unwrap();
}

#[test]
fn show_of_a_map_name_prints_the_map() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    add(add_args(&["concept", "Snapshot"]), &log, &schemas, &source("cli"), no_checkout(), human(), None)
        .unwrap();

    show(show_args("concepts"), &log, &schemas, Path::new(ROOT)).unwrap();
}

#[test]
fn a_flag_that_does_not_fit_the_resolved_form_is_refused() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();

    let args = ShowArgs { depth: Some(2), ..ShowArgs::default() };
    let err = show(args, &log, &schemas, Path::new(ROOT)).unwrap_err();

    assert!(err.to_string().contains("--depth"), "{err}");
}

/// `AddArgs` from a bare word list - the shape `parse_write_args` reads.
fn add_args(tokens: &[&str]) -> AddArgs {
    AddArgs { args: tokens.iter().map(|s| s.to_string()).collect() }
}

/// `RemoveArgs`, the same way as `add_args`.
fn remove_args(tokens: &[&str]) -> RemoveArgs {
    RemoveArgs { args: tokens.iter().map(|s| s.to_string()).collect() }
}

#[test]
fn add_captures_its_whole_tail_including_flags() {
    let cli =
        Cli::try_parse_from(["percept", "add", "concept", "Snapshot", "--definition", "x"]).unwrap();
    let Some(Command::Add(args)) = cli.command else {
        panic!("expected add")
    };
    assert_eq!(args.args, vec!["concept", "Snapshot", "--definition", "x"]);
}

#[test]
fn add_and_remove_fail_on_a_kind_no_schema_declares() {
    let log = FakeLog::default();
    let cli_source = source("cli");

    let added = add(
        add_args(&["file", "src/main.rs"]),
        &log,
        &schemas(),
        &cli_source,
        no_checkout(),
        human(),
        None,
    );
    let removed = remove(
        remove_args(&["file", "src/main.rs"]),
        &log,
        &schemas(),
        &cli_source,
        human(),
        None,
    );

    for result in [added, removed] {
        let err = result.err().unwrap();
        assert!(err.to_string().starts_with("no map declares \"file\""), "{err}");
    }
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn add_commits_as_the_actor_given_and_defaults_to_human() {
    let log = FakeLog::default();
    add(
        add_args(&["topic", "Which?", "--actor", "agent"]),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();
    assert!(log.load().unwrap().last().unwrap().actor() == Actor::Agent);

    let log = FakeLog::default();
    add(
        add_args(&["topic", "Which too?"]),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();
    assert!(matches!(log.load().unwrap().last().unwrap().actor(), Actor::Human(_)));
}

#[test]
fn add_carries_the_cause_it_is_given() {
    let log = FakeLog::default();
    let cause = crate::core::EventId::new();

    add(
        add_args(&["topic", "Which?"]),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        Some(cause),
    )
    .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.last().unwrap().causation_id(), Some(cause));
}

#[test]
fn an_explicit_causation_flag_wins_over_the_turns() {
    let log = FakeLog::default();
    let turns_cause = crate::core::EventId::new();
    let seed = Event::message_received(Actor::Human(human()), "hi".to_string(), source("cli"), None);
    let explicit = seed.id();
    log.append(&seed).unwrap();
    let flag = explicit.as_uuid().to_string();

    add(
        add_args(&["topic", "Which?", "--causation", &flag]),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        Some(turns_cause),
    )
    .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.last().unwrap().causation_id(), Some(explicit));
}

#[test]
fn node_written_line_names_the_maps_level() {
    let schemas = concepts_and_projects();
    assert_eq!(node_written_line(&schemas, "concepts", "c3"), "c3  concepts (project)");
    assert_eq!(node_written_line(&schemas, "projects", "p1"), "p1  projects (~)");
}

#[test]
fn document_node_line_names_the_node_before_its_map() {
    let schemas = concepts_and_projects();
    assert_eq!(
        document_node_line(&schemas, "concepts", "c3", "concept", "Doc"),
        "c3 concept \"Doc\"  concepts (project)"
    );
}

#[test]
fn a_node_added_to_a_global_schema_lands_at_home() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();

    add(
        add_args(&["project", "percept"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    let created = events
        .iter()
        .find(|event| matches!(event.payload(), Payload::MapCreated { .. }))
        .expect("a first write to a map creates it");
    assert_eq!(created.source().path, Path::new(HOME));
}

#[test]
fn a_node_added_to_a_project_schema_lands_at_the_project_root() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();

    add(
        add_args(&["concept", "Snapshot"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    let created = events
        .iter()
        .find(|event| matches!(event.payload(), Payload::MapCreated { .. }))
        .expect("a first write to a map creates it");
    assert_eq!(created.source().path, Path::new(ROOT));
}

#[test]
fn an_edge_across_two_maps_is_refused_naming_both() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    for tokens in [["concept", "Snapshot"], ["project", "percept"]] {
        add(add_args(&tokens), &log, &schemas, &source("cli"), no_checkout(), human(), None).unwrap();
    }

    let err = add(
        add_args(&["relates", "c1", "p1"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .err()
    .unwrap();

    assert!(err.to_string().contains("concepts"), "{err}");
    assert!(err.to_string().contains("projects"), "{err}");
}

#[test]
fn an_edge_between_two_nodes_of_one_map_resolves_the_map_with_no_name() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    for tokens in [["concept", "Snapshot"], ["concept", "Undo"]] {
        add(add_args(&tokens), &log, &schemas, &source("cli"), no_checkout(), human(), None).unwrap();
    }

    add(
        add_args(&["relates", "c1", "c2"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let map = mapstore::fold_map(&log, &schemas, "concepts", Path::new(ROOT)).unwrap();
    assert_eq!(map.edges().len(), 1);
    assert_eq!(map.edges()[0].kind, "relates");
}

#[test]
fn an_unknown_property_flag_is_refused() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();

    let err = add(
        add_args(&["concept", "Snapshot", "--nonsense", "value"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .err()
    .unwrap();

    assert!(err.to_string().contains("nonsense"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn remove_drops_a_node_and_an_edge() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    for tokens in [["concept", "Snapshot"], ["concept", "Undo"]] {
        add(add_args(&tokens), &log, &schemas, &source("cli"), no_checkout(), human(), None).unwrap();
    }
    add(
        add_args(&["relates", "c1", "c2"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    remove(remove_args(&["relates", "c1", "c2"]), &log, &schemas, &source("cli"), human(), None).unwrap();
    let map = mapstore::fold_map(&log, &schemas, "concepts", Path::new(ROOT)).unwrap();
    assert!(map.edges().is_empty());

    remove(remove_args(&["concept", "c1"]), &log, &schemas, &source("cli"), human(), None).unwrap();
    let map = mapstore::fold_map(&log, &schemas, "concepts", Path::new(ROOT)).unwrap();
    assert!(map.find("concept", "Snapshot").is_none());
}

/// `ChangeArgs` from a bare word list - the shape `parse_write_args`
/// reads, the same as `add_args`/`remove_args`.
fn change_args(tokens: &[&str]) -> ChangeArgs {
    ChangeArgs { args: tokens.iter().map(|s| s.to_string()).collect() }
}

#[test]
fn change_with_a_property_sets_the_nodes_why_property() {
    let added = node_added_by(Actor::Agent, "verdict", "Rust");
    let node = node_id(&added);
    let log = FakeLog::seeded(vec![map_created("debates"), added]);

    change(
        change_args(&["verdict:Rust", "--why", "never proposed"]),
        &log,
        &schemas(),
        &source("cli"),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    match events[2].payload() {
        Payload::NodeChanged {
            node: changed,
            properties,
            ..
        } => {
            assert!(*changed == node);
            assert_eq!(properties.get("why").map(String::as_str), Some("never proposed"));
        }
        _ => panic!("expected NodeChanged"),
    }
    assert!(matches!(events[2].actor(), Actor::Human(_)));
}

#[test]
fn change_refuses_a_node_the_map_does_not_hold() {
    let log = FakeLog::default();

    let err = change(
        change_args(&["verdict:Rust", "--why", "never proposed"]),
        &log,
        &schemas(),
        &source("cli"),
        human(),
        None,
    )
    .err()
    .unwrap();

    assert!(err.to_string().contains("no verdict"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn change_refuses_an_agents_property_change_of_a_node_a_human_wrote() {
    let added = node_added("claim", "wasm render");
    let log = FakeLog::seeded(vec![map_created("debates"), added]);

    let err = change(
        change_args(&["claim:wasm render", "--actor", "agent", "--why", "faster paint"]),
        &log,
        &schemas(),
        &source("cli"),
        None,
        None,
    )
    .err()
    .unwrap();

    assert!(err.to_string().contains("was written by"), "{err}");
    assert_eq!(log.load().unwrap().len(), 2);
}

#[test]
fn change_of_a_node_in_a_global_map_lands_at_home() {
    let log = FakeLog::default();
    let schemas = concepts_and_projects();
    add(
        add_args(&["project", "percept"]),
        &log,
        &schemas,
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    change(
        change_args(&["p1", "--root", "~/code/percept"]),
        &log,
        &schemas,
        &source("cli"),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    assert!(events.iter().any(|event| matches!(event.payload(), Payload::NodeChanged { .. })));
    // `change` found the map `add` already created at `HOME`, rather
    // than minting a second one at `ROOT`: exactly one `map.created`
    // event, rooted at `HOME`.
    let created: Vec<&Event> = events
        .iter()
        .filter(|event| matches!(event.payload(), Payload::MapCreated { .. }))
        .collect();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].source().path, Path::new(HOME));
}

#[test]
fn a_document_writes_its_nodes_and_edges_in_order() {
    let log = FakeLog::default();
    let document = "verdict \"yes\"\n  why \"it ran\"\n\
                     topic \"Does record work?\"\n  settles verdict\n";
    record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.len(), 4);
    let map = mapstore::fold_map(&log, &schemas(), "debates", Path::new(ROOT)).unwrap();
    assert!(map.find("topic", "Does record work?").is_some());
    let verdict = map.find("verdict", "yes").unwrap();
    assert_eq!(verdict.properties.get("why").unwrap(), "it ran");
    assert_eq!(map.edges().len(), 1);
    assert_eq!(map.edges()[0].kind, "settles");
}

#[test]
fn a_record_takes_the_turns_cause_when_none_is_given() {
    let log = FakeLog::default();
    let cause = crate::core::EventId::new();
    let fixture = Fixture::new();
    fixture.write("src/cli/mod.rs", "one\ntwo\nthree\n");
    let document = "topic \"Does record work?\"\n  cites src/cli/mod.rs:1-3\n";

    record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        fixture.path(),
        human(),
        Some(cause),
    )
    .unwrap();

    let events = log.load().unwrap();
    assert!(!events.is_empty());
    for event in events.iter().skip(1) {
        assert_eq!(event.causation_id(), Some(cause));
    }
}

#[test]
fn an_explicit_causation_wins_over_the_turns() {
    let log = FakeLog::default();
    let turns_cause = crate::core::EventId::new();
    let seed = Event::message_received(Actor::Human(human()), "hi".to_string(), source("cli"), None);
    let explicit = seed.id();
    log.append(&seed).unwrap();
    let mut args = write_args();
    args.causation = Some(explicit.as_uuid().to_string());
    let document = "topic \"Does record work?\"\n";

    record_document(
        document,
        args,
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        Some(turns_cause),
    )
    .unwrap();

    let events = log.load().unwrap();
    let recorded = events.last().unwrap();
    assert_eq!(recorded.causation_id(), Some(explicit));
}

#[test]
fn a_cites_line_publishes_a_file_cited_event_and_cites_it() {
    let fixture = Fixture::new();
    fixture.write("src/cli/mod.rs", "one\ntwo\nthree\n");
    let log = FakeLog::default();
    let document = "verdict \"yes\"\n  why \"it ran\"\n  cites src/cli/mod.rs:1-3\n\
                     topic \"Does record work?\"\n  settles verdict\n";
    record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        fixture.path(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    // The `cites` event is published before the node it cites - `record`
    // writes a node's cites first - so it lands right after the map is
    // created, ahead of the verdict node it belongs to.
    assert!(matches!(events[1].payload(), Payload::FileCited { .. }));
    let cite_id = events[1].id();
    let map = mapstore::fold_map(&log, &schemas(), "debates", Path::new(ROOT)).unwrap();
    let verdict = map.find("verdict", "yes").unwrap();
    assert!(verdict.sources.contains(&cite_id));
}

#[test]
fn a_cites_line_under_a_short_id_reaches_the_node_it_names() {
    let fixture = Fixture::new();
    fixture.write("src/cli/mod.rs", "one\ntwo\nthree\n");
    let log = FakeLog::default();
    record_document(
        "topic \"Does record work?\"\n",
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        fixture.path(),
        human(),
        None,
    )
    .unwrap();

    record_document(
        "t1\n  cites src/cli/mod.rs:1-3\n",
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        fixture.path(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    let cite_id = events
        .iter()
        .find(|event| matches!(event.payload(), Payload::FileCited { .. }))
        .expect("the document cited a file")
        .id();
    let map = mapstore::fold_map(&log, &schemas(), "debates", Path::new(ROOT)).unwrap();
    let topic = map.find("topic", "Does record work?").unwrap();
    assert!(topic.sources.contains(&cite_id), "{:?}", topic.sources);
}

#[test]
fn a_ref_to_an_existing_short_id_resolves() {
    let log = FakeLog::default();
    log.append(&map_created("debates")).unwrap();
    log.append(&node_added("topic", "Does record work?")).unwrap();
    // `doubts` runs from a verdict to a topic, so the new verdict is
    // the doc's current node - the `from` end the syntax always gives
    // the enclosing line - and the existing topic, `t1`, is its `to`.
    let document = "verdict \"yes\"\n  why \"it ran\"\n  doubts t1\n";
    record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let map = mapstore::fold_map(&log, &schemas(), "debates", Path::new(ROOT)).unwrap();
    assert_eq!(map.edges().len(), 1);
    assert_eq!(map.edges()[0].kind, "doubts");
}

#[test]
fn an_unknown_node_kind_fails_before_anything_is_written() {
    let log = FakeLog::default();
    let document = "riddle \"what?\"\n";
    let err = record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("riddle"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_closed_list_property_left_unset_records_with_no_value() {
    let log = FakeLog::default();
    let document = "chore \"cancel a turn\"\n  why \"Esc drops the session\"\n";
    record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    match events.last().unwrap().payload() {
        Payload::NodeAdded { properties, .. } => {
            assert_eq!(properties.get("state"), None);
        }
        _ => panic!("expected NodeAdded"),
    }
}

#[test]
fn a_bad_ref_names_its_line_number() {
    let log = FakeLog::default();
    let document = "topic \"Does record work?\"\n\
                     verdict \"yes\"\n  why \"it ran\"\n  settles t9\n";
    let err = record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().starts_with("line 4:"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_duplicate_name_in_a_later_node_writes_nothing() {
    let log = FakeLog::default();
    let document = "topic \"Does record work?\"\n\
                     topic \"Does record work?\"\n";
    let err = record_document(
        document,
        write_args(),
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("already in the map"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_source_id_the_log_lacks_writes_nothing() {
    let log = FakeLog::default();
    let document = "topic \"Does record work?\"\n";
    let mut args = write_args();
    args.source = vec![crate::core::EventId::new().as_uuid().to_string()];
    let err = record_document(
        document,
        args,
        &log,
        &schemas(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("no event with id"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_short_id_document_changes_the_node_and_records_node_changed() {
    let log = FakeLog::default();
    record_document(
        "chore \"cancel a turn\"\n  why \"Esc drops the session\"\n  state \"open\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    record_document(
        "c1\n  state \"done\"\n  outcome \"1f1a9a9: done\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    assert!(matches!(
        events.last().unwrap().payload(),
        Payload::NodeChanged { .. }
    ));

    let map = mapstore::fold_map(&log, &chores_only(), "chores", Path::new(ROOT)).unwrap();
    let chore = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(chore.properties.get("state").unwrap(), "done");
    assert_eq!(chore.properties.get("outcome").unwrap(), "1f1a9a9: done");
}

#[test]
fn a_change_block_carrying_only_an_edge_records_no_node_change() {
    let log = FakeLog::default();
    record_document(
        "chore \"first\"\n  why \"it came up\"\n  state \"open\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let mut agent = write_args();
    agent.actor = "agent".to_string();
    record_document(
        "chore \"second\"\n  why \"it follows\"\n  state \"open\"\nc1\n  blocks c2\n",
        agent,
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event.payload(), Payload::NodeChanged { .. })),
        "an edge under a short id changed nothing about the node it names"
    );

    let map = mapstore::fold_map(&log, &chores_only(), "chores", Path::new(ROOT)).unwrap();
    let first = map.find("chore", "first").unwrap();
    let children = map.children(first.id);
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].0, "blocks");
    assert_eq!(children[0].1.name, "second");
}

#[test]
fn a_short_id_line_with_a_quoted_name_is_an_error() {
    let log = FakeLog::default();
    let err = record_document(
        "c4 \"name\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().starts_with("line 1:"), "{err}");
}

#[test]
fn a_change_to_an_unknown_short_id_is_an_error() {
    let log = FakeLog::default();
    let err = record_document(
        "c9\n  state \"done\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("c9"), "{err}");
    assert!(log.load().unwrap().is_empty());
}

#[test]
fn a_record_why_line_under_a_change_block_sets_the_nodes_why_property() {
    let log = FakeLog::default();
    record_document(
        "chore \"cancel a turn\"\n  why \"Esc drops the session\"\n  state \"open\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    record_document(
        "c1\n  why \"never proposed\"\n",
        write_args(),
        &log,
        &chores_only(),
        &source("cli"),
        no_checkout(),
        human(),
        None,
    )
    .unwrap();

    let events = log.load().unwrap();
    match events.last().unwrap().payload() {
        Payload::NodeChanged { properties, .. } => {
            assert_eq!(properties.get("why").map(String::as_str), Some("never proposed"));
        }
        _ => panic!("expected NodeChanged"),
    }

    let map = mapstore::fold_map(&log, &chores_only(), "chores", Path::new(ROOT)).unwrap();
    let chore = map.find("chore", "cancel a turn").unwrap();
    assert_eq!(chore.properties.get("why").unwrap(), "never proposed");
}
