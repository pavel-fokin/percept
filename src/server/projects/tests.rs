use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::core::testing::{created_at, human, FakeLog, Fixture};
use crate::core::{Actor, Event, NodeId, Payload, Source};
use crate::shared::Timestamp;

const SCHEMA: &str = "name = \"decisions\"\npurpose = \"test\"\nheadlines = [\"concept\"]\n\n[[node]]\nkind = \"concept\"\n";

fn source_at(name: &str, path: &Path) -> Source {
    Source {
        name: name.to_string(),
        path: path.to_path_buf(),
    }
}

fn node_added(path: &Path, name: &str) -> Event {
    node_added_seq(path, name, 1)
}

/// `node_added`, numbered `seq` - for a test that adds more than one
/// `concept` node under the same path, where each needs the number
/// `apply` would have minted for it.
fn node_added_seq(path: &Path, name: &str, seq: u32) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at("agent", path),
        None,
        Payload::NodeAdded {
            map: crate::core::testing::map_id("decisions"),
            node: NodeId::new(),
            kind: "concept".to_string(),
            name: name.to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
            seq,
        },
    )
}

fn session_started(path: &Path) -> Event {
    Event::session_started(source_at("agent", path))
}

fn map_created(path: &Path) -> Event {
    Event::map_created(
        crate::core::testing::map_id("decisions"),
        "decisions".to_string(),
        source_at("percept", path),
    )
}

/// A view opened after everything the test seeded, so every session in
/// the fixture counts as having begun before it.
fn after_everything() -> Timestamp {
    Timestamp::now().minus_minutes(-60).unwrap()
}

#[test]
fn a_project_with_no_schemas_still_appears_with_no_maps() {
    let fixture = Fixture::new();
    let log = FakeLog::seeded(vec![node_added(fixture.path(), "no schema here")]);

    let body = list(&log, after_everything()).unwrap();

    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["maps"].as_array().unwrap().len(), 0);
}

#[test]
fn a_project_reports_its_own_event_count_and_root_name() {
    let fixture = Fixture::new();
    let events = vec![node_added_seq(fixture.path(), "a", 1), node_added_seq(fixture.path(), "b", 2)];
    let log = FakeLog::seeded(events);

    let body = list(&log, after_everything()).unwrap();

    let project = &body["projects"][0];
    assert_eq!(project["events"], 2);
    assert_eq!(project["name"], fixture.path().file_name().unwrap().to_string_lossy().to_string());
    assert_eq!(project["path"], fixture.path().to_string_lossy().to_string());
}

#[test]
fn projects_are_ordered_newest_last_active_first() {
    let older = Fixture::new();
    let newer = Fixture::new();
    let older_event = created_at(node_added(older.path(), "old"), Timestamp::now().minus_minutes(60).unwrap());
    let newer_event = created_at(node_added(newer.path(), "new"), Timestamp::now());
    let log = FakeLog::seeded(vec![older_event, newer_event]);

    let body = list(&log, after_everything()).unwrap();

    let projects = body["projects"].as_array().unwrap();
    assert_eq!(projects[0]["path"], newer.path().to_string_lossy().to_string());
    assert_eq!(projects[1]["path"], older.path().to_string_lossy().to_string());
}

#[test]
fn a_map_reports_its_name_and_node_count() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/decisions.toml", SCHEMA);
    let log = FakeLog::seeded(vec![map_created(fixture.path()), node_added(fixture.path(), "why blue?")]);

    let body = list(&log, after_everything()).unwrap();

    let maps = body["projects"][0]["maps"].as_array().unwrap();
    assert_eq!(maps.len(), 1);
    assert_eq!(
        maps[0]["id"],
        crate::core::testing::map_id("decisions")
            .as_uuid()
            .to_string()
    );
    assert_eq!(maps[0]["name"], "decisions");
    assert_eq!(maps[0]["nodes"], 1);
}

#[test]
fn gained_counts_only_nodes_changed_at_or_after_the_last_session() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/decisions.toml", SCHEMA);
    let session = session_started(fixture.path());
    let since = session.created_at();
    let before = created_at(
        node_added_seq(fixture.path(), "before the session", 1),
        since.minus_minutes(60).unwrap(),
    );
    let after = created_at(
        node_added_seq(fixture.path(), "after the session", 2),
        since.minus_minutes(-10).unwrap(),
    );
    let log = FakeLog::seeded(vec![map_created(fixture.path()), before, session, after]);

    let body = list(&log, after_everything()).unwrap();

    let maps = body["projects"][0]["maps"].as_array().unwrap();
    assert_eq!(maps[0]["nodes"], 2);
    assert_eq!(maps[0]["gained"], 1);
}

#[test]
fn a_session_started_after_this_view_opened_is_not_the_baseline() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/decisions.toml", SCHEMA);
    let opened = Timestamp::now();
    let earlier = created_at(session_started(fixture.path()), opened.minus_minutes(60).unwrap());
    let node = created_at(node_added(fixture.path(), "recorded since"), opened.minus_minutes(30).unwrap());
    // What `percept web` itself records by opening: a session newer
    // than every node, which read live would leave nothing gained.
    let own = created_at(session_started(fixture.path()), opened);
    let log = FakeLog::seeded(vec![map_created(fixture.path()), earlier, node, own]);

    let body = list(&log, opened).unwrap();

    assert_eq!(body["projects"][0]["maps"][0]["gained"], 1);
}

#[test]
fn gained_is_zero_when_the_project_has_no_session_recorded() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/decisions.toml", SCHEMA);
    let log = FakeLog::seeded(vec![map_created(fixture.path()), node_added(fixture.path(), "no session yet")]);

    let body = list(&log, after_everything()).unwrap();

    assert_eq!(body["projects"][0]["maps"][0]["gained"], 0);
}

#[test]
fn a_project_whose_schemas_cannot_be_read_says_why_and_leaves_the_others_listed() {
    let broken = Fixture::new();
    broken.write(".percept/schemas/decisions.toml", "name = \"decisions\"\n");
    let sound = Fixture::new();
    sound.write(".percept/schemas/decisions.toml", SCHEMA);
    let events = vec![
        map_created(broken.path()),
        node_added(broken.path(), "a"),
        map_created(sound.path()),
        node_added(sound.path(), "b"),
    ];
    let log = FakeLog::seeded(events);

    let body = list(&log, after_everything()).unwrap();

    let projects = body["projects"].as_array().unwrap();
    let broken = projects.iter().find(|p| p["path"] == broken.path().to_string_lossy().to_string()).unwrap();
    let sound = projects.iter().find(|p| p["path"] == sound.path().to_string_lossy().to_string()).unwrap();
    assert!(broken["maps_error"].as_str().unwrap().contains("decisions.toml"));
    assert_eq!(broken["maps"].as_array().unwrap().len(), 0);
    assert_eq!(sound["maps_error"], serde_json::Value::Null);
    assert_eq!(sound["maps"][0]["name"], "decisions");
}
