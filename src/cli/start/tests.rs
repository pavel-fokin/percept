use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::core::testing::{
    created_at, human, node_added, node_added_citing, node_id, schemas, source_at, FakeLog, ROOT,
};
use crate::core::{Actor, Event, EventId, NodeId};
use crate::mapstore;

fn root() -> PathBuf {
    PathBuf::from(ROOT)
}

/// `events`, folded exactly as `start::start` folds them: every schema,
/// cut to `root`'s own path.
fn fold(events: &[Event]) -> Vec<Map> {
    schemas().fold_all(mapstore::of_path(events, &root())).unwrap()
}

/// A `node.added` event for `map`, with `properties` - what a task's
/// `why` and `state` need, which `core::testing::node_added` doesn't
/// carry since it always writes to the decisions map.
fn node_on(map: &str, kind: &str, name: &str, properties: BTreeMap<String, String>) -> Event {
    Event::new(
        Actor::Human(human()),
        source_at("test", ROOT),
        None,
        Payload::NodeAdded {
            map: map.to_string(),
            node: NodeId::new(),
            kind: kind.to_string(),
            name: name.to_string(),
            properties,
            sources: Vec::new(),
            seq: 0,
        },
    )
}

/// A `node.changed` event for `node`, carrying only `why`, at `at` - a
/// human's correction for an Attention test to compare against.
fn changed_by_human(map: &str, node: &Event, why: &str, at: Timestamp) -> Event {
    Event::restore(
        EventId::new(),
        Actor::Human(human()),
        source_at("test", ROOT),
        None,
        at,
        Payload::NodeChanged {
            map: map.to_string(),
            node: node_id(node),
            name: None,
            properties: BTreeMap::new(),
            sources: Vec::new(),
            why: Some(why.to_string()),
        },
    )
}

/// A `file.cited` event citing `path`, repo-relative, with `excerpt` as
/// its text.
fn citation(path: &str, lines: Option<(u32, u32)>, excerpt: &str) -> Event {
    Event::new(
        Actor::Agent,
        source_at("test", ROOT),
        None,
        Payload::FileCited {
            path: PathBuf::from(path),
            lines,
            excerpt: excerpt.to_string(),
        },
    )
}

fn scratch_checkout() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

#[test]
fn nothing_recorded_prints_the_empty_state_and_only_a_describe_pointer() {
    let text = render(&fold(&[]), &[], &root(), scratch_checkout().path(), None);

    assert_eq!(
        text,
        "percept \u{b7} test\nnothing recorded yet\n\n\
         Next\n  how to record   percept maps describe <map>"
    );
}

#[test]
fn the_state_line_shows_a_maps_headline_count() {
    let events = vec![node_added("question", "why blue?")];
    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("decisions   1"), "{text:?}");
}

#[test]
fn a_session_from_another_client_at_this_path_sets_the_since_count() {
    let session = Event::session_started(source_at("claude-code", ROOT));
    let since = session.created_at();
    let after = since.minus_minutes(-10).unwrap();
    let events = vec![session, created_at(node_added("question", "fresh"), after)];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("+1 since last session"), "{text:?}");
}

#[test]
fn a_session_at_a_different_path_does_not_set_the_since_count() {
    let session = Event::session_started(source_at("claude-code", "/elsewhere"));
    let events = vec![session, node_added("question", "fresh")];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(!text.contains("since last session"), "{text:?}");
}

#[test]
fn one_open_task_shows_its_state_count() {
    let mut properties = BTreeMap::new();
    properties.insert("why".to_string(), "it matters".to_string());
    properties.insert("state".to_string(), "open".to_string());
    let events = vec![node_on("tasks", "task", "ship it", properties)];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("1 open"), "{text:?}");
}

#[test]
fn attention_marks_a_fresh_node_as_added() {
    let session = Event::session_started(source_at("claude-code", ROOT));
    let since = session.created_at();
    let after = since.minus_minutes(-10).unwrap();
    let events = vec![session, created_at(node_added("question", "fresh one"), after)];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("question \"fresh one\""), "{text:?}");
    assert!(text.contains("added"), "{text:?}");
}

#[test]
fn attention_marks_a_node_changed_by_a_human_with_who_and_why() {
    let session = Event::session_started(source_at("claude-code", ROOT));
    let since = session.created_at();
    let earlier = since.minus_minutes(60).unwrap();
    let after = since.minus_minutes(-10).unwrap();

    let decision = created_at(node_added("decision", "old one"), earlier);
    let change = changed_by_human("decisions", &decision, "still hurts", after);
    let events = vec![session, decision, change];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("decision \"old one\""), "{text:?}");
    assert!(text.contains("changed by human: \"still hurts\""), "{text:?}");
}

#[test]
fn attention_marks_a_stale_citation_as_changed() {
    let checkout = scratch_checkout();
    std::fs::write(checkout.path().join("a.rs"), "fn one() { edited }\n").unwrap();

    let cited = citation("a.rs", Some((1, 1)), "fn one() {}");
    let node = node_added_citing(Actor::Human(human()), "question", "why a?", vec![cited.id()]);
    let events = vec![cited, node];

    let text = render(&fold(&events), &events, &root(), checkout.path(), last_session_at(&events, &root()));

    assert!(text.contains("cites a.rs:1-1"), "{text:?}");
    assert!(text.contains("changed"), "{text:?}");
}

#[test]
fn next_offers_a_read_around_for_every_attention_id() {
    let checkout = scratch_checkout();
    std::fs::write(checkout.path().join("a.rs"), "fn one() { edited }\n").unwrap();

    let cited = citation("a.rs", Some((1, 1)), "fn one() {}");
    let node = node_added_citing(Actor::Human(human()), "question", "why a?", vec![cited.id()]);
    let events = vec![cited, node];

    let text = render(&fold(&events), &events, &root(), checkout.path(), last_session_at(&events, &root()));

    assert!(text.contains("read around q1"), "{text:?}");
    assert!(text.contains("percept maps show decisions --around q1"), "{text:?}");
}

#[test]
fn next_has_no_read_line_for_a_map_with_no_headline() {
    let events = vec![node_added("question", "why?")];

    let text = render(&fold(&events), &events, &root(), scratch_checkout().path(), last_session_at(&events, &root()));

    assert!(text.contains("percept maps show decisions"), "{text:?}");
    assert!(!text.contains("percept maps show tasks"), "{text:?}");
}

#[test]
fn start_reads_and_appends_no_event() {
    let log = FakeLog::seeded(vec![node_added("question", "why?")]);
    let schemas = schemas();
    let checkout = scratch_checkout();

    start(&log, &schemas, &root(), checkout.path()).unwrap();

    assert_eq!(log.load().unwrap().len(), 1);
}
