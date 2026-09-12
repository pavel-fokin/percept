use serde_json::Value;

use super::*;
use crate::core::testing::{
    created_at, edge_added, file_cited, human, node_added_by, node_added_citing, schemas, source,
    source_at, FakeLog,
};
use crate::core::{Actor, Event, EventId, Payload};
use crate::shared::Timestamp;

fn message_received(actor: Actor, content: &str) -> Event {
    Event::new(actor, source("test"), None, Payload::MessageReceived { content: content.to_string() })
}

fn message_received_at(source: Source, actor: Actor, content: &str) -> Event {
    Event::new(actor, source, None, Payload::MessageReceived { content: content.to_string() })
}

fn cut_body_since(events: Vec<Event>, since: Option<Timestamp>) -> Value {
    let log = FakeLog::seeded(events);
    let schemas = schemas();
    let src = source("test");
    serde_json::to_value(cut(&log, &schemas, &src, since).unwrap()).unwrap()
}

fn cut_debates(events: Vec<Event>) -> Value {
    cut_debates_since(events, None)
}

fn cut_debates_since(events: Vec<Event>, since: Option<Timestamp>) -> Value {
    let body = cut_body_since(events, since);
    body["maps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|map| map["name"] == "debates")
        .unwrap()
        .clone()
}

fn groups_of(map: &Value) -> &Vec<Value> {
    map["groups"].as_array().unwrap()
}

/// `group`'s claims - the one path most tests need past `groups_of`.
fn claims_of(map: &Value, group: usize) -> &Vec<Value> {
    groups_of(map)[group]["claims"].as_array().unwrap()
}

/// The first group's first claim's sources - what a test about one
/// source reaches for.
fn first_sources(map: &Value) -> &Vec<Value> {
    claims_of(map, 0)[0]["sources"].as_array().unwrap()
}

#[test]
fn a_claimed_verdict_appears_under_the_topic_it_settles() {
    let topic = node_added_by(Actor::Agent, "topic", "Which language?");
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, verdict, settles]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["heading"]["title"], "Which language?");
    let claims = claims_of(&map, 0);
    assert_eq!(claims.len(), 2, "the topic is a row of its own group too");
    assert_eq!(claims[0]["name"], "Which language?");
    assert_eq!(claims[1]["name"], "Rust");
}

#[test]
fn cut_with_a_since_after_a_nodes_changed_at_leaves_it_out() {
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");
    let t0 = Timestamp::now();
    let seeded = created_at(verdict, t0);
    let since = t0.minus_minutes(-10).unwrap();

    let map = cut_debates_since(vec![seeded], Some(since));

    assert!(groups_of(&map).is_empty(), "{map:?}");
}

#[test]
fn cut_with_no_since_includes_every_headline_node() {
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");

    let map = cut_debates(vec![verdict]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(claims_of(&map, 0)[0]["name"], "Rust");
}

#[test]
fn a_since_at_or_before_a_nodes_changed_at_keeps_it_in_the_cut() {
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");
    let t0 = Timestamp::now();
    let seeded = created_at(verdict, t0);

    let map = cut_debates_since(vec![seeded], Some(t0));

    assert_eq!(claims_of(&map, 0)[0]["name"], "Rust");
}

#[test]
fn a_reopening_topic_groups_under_the_verdict_it_doubts() {
    // No edge kind sorts a group ahead of another: groups order by
    // their heading's `added_at`, so the older, unrelated topic
    // still comes first.
    let older_topic = node_added_by(Actor::Agent, "topic", "Older topic?");
    let verdict = node_added_by(Actor::Agent, "verdict", "Something");
    let newer_topic = node_added_by(Actor::Agent, "topic", "Newer, reopening?");
    let doubts = edge_added("doubts", &newer_topic, &verdict);

    let map = cut_debates(vec![older_topic, verdict, newer_topic, doubts]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 2);
    assert_eq!(claims_of(&map, 0)[0]["name"], "Older topic?");
    assert_eq!(groups[1]["heading"]["title"], "Something");
    assert_eq!(claims_of(&map, 1)[1]["name"], "Newer, reopening?");
}

#[test]
fn an_open_topic_with_no_verdict_is_its_own_group_with_a_null_heading() {
    let topic = node_added_by(Actor::Agent, "topic", "Which language?");

    let map = cut_debates(vec![topic]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["heading"].is_null(), "{:?}", groups[0]);
    let claims = claims_of(&map, 0);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["name"], "Which language?");
}

#[test]
fn a_verdict_that_replaces_another_groups_under_that_one_not_its_topic() {
    // The heading search is one hop, checking edge kinds in schema
    // order: `correction` has no `settles` edge of its own, so it
    // groups under `old_verdict`, the node its `replaces` edge
    // reaches - the core keeps no supersession chain for the review to
    // follow further.
    let topic = node_added_by(Actor::Agent, "topic", "Which language?");
    let old_verdict = node_added_by(Actor::Agent, "verdict", "Rust");
    let settles = edge_added("settles", &old_verdict, &topic);
    let correction = node_added_by(Actor::Agent, "verdict", "Go");
    let replaces = edge_added("replaces", &correction, &old_verdict);

    let map = cut_debates(vec![topic, old_verdict, settles, correction, replaces]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["heading"]["title"], "Which language?");
    assert_eq!(claims_of(&map, 0)[1]["name"], "Rust");
    assert_eq!(groups[1]["heading"]["title"], "Rust");
    assert_eq!(claims_of(&map, 1)[0]["name"], "Go");
}

#[test]
fn an_orphan_group_carries_a_null_heading() {
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");

    let map = cut_debates(vec![verdict]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["heading"].is_null(), "{:?}", groups[0]);
}

#[test]
fn a_claim_about_the_topic_is_related_under_the_verdict_row() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let verdict = node_added_by(Actor::Agent, "verdict", "Rust");
    let settles = edge_added("settles", &verdict, &topic);
    let claim = node_added_by(Actor::Agent, "claim", "Go");
    let about = edge_added("about", &claim, &topic);

    let map = cut_debates(vec![topic, verdict, settles, claim, about]);

    let claims = claims_of(&map, 0);
    let options = claims[0]["related"].as_array().unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0]["name"], "Go");
    assert_eq!(options[0]["changed_by"], "agent");
}

#[test]
fn a_verdict_citing_a_human_prompt_carries_the_prompts_content_and_the_agent_reply_before_it_as_its_proposal(
) {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let t0 = Timestamp::now();
    let proposal = created_at(message_received(Actor::Agent, "Recommendation: Rust."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Yes, Rust."), t1);
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![prompt.id()]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, proposal, prompt, verdict, settles]);

    let sources = first_sources(&map);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["kind"], "message");
    assert_eq!(sources[0]["content"], "Yes, Rust.");
    assert_eq!(sources[0]["proposal"]["content"], "Recommendation: Rust.");
}

#[test]
fn a_prompt_with_no_earlier_agent_reply_in_its_source_carries_a_null_proposal() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let prompt = message_received(Actor::Human(human()), "Rust, please.");
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![prompt.id()]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, prompt, verdict, settles]);

    let sources = first_sources(&map);
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_reply_from_another_source_is_not_taken_as_the_proposal() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let t0 = Timestamp::now();
    let other_reply = created_at(message_received_at(source_at("other", "/other"), Actor::Agent, "Go."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Rust, please."), t1);
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![prompt.id()]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, other_reply, prompt, verdict, settles]);

    let sources = first_sources(&map);
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_file_cited_source_carries_its_path_lines_and_excerpt() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let cite = file_cited("src/main.rs", Some((10, 20)), "fn main() {}");
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![cite.id()]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, cite, verdict, settles]);

    let sources = first_sources(&map);
    assert_eq!(sources[0]["kind"], "file");
    assert_eq!(sources[0]["path"], "src/main.rs");
    assert_eq!(sources[0]["lines"][0], 10);
    assert_eq!(sources[0]["lines"][1], 20);
    assert_eq!(sources[0]["label"], "src/main.rs:10-20");
    assert_eq!(sources[0]["excerpt"], "fn main() {}");
}

#[test]
fn a_source_id_the_log_does_not_hold_reads_as_missing() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let missing_id = EventId::new();
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![missing_id]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, verdict, settles]);

    let sources = first_sources(&map);
    assert_eq!(sources[0]["kind"], "missing");
}

#[test]
fn content_over_4000_characters_is_cut_and_marked_truncated() {
    let topic = node_added_by(Actor::Human(None), "topic", "Which language?");
    let long = "a".repeat(4001);
    let prompt = message_received(Actor::Human(human()), &long);
    let verdict = node_added_citing(Actor::Agent, "verdict", "Rust", vec![prompt.id()]);
    let settles = edge_added("settles", &verdict, &topic);

    let map = cut_debates(vec![topic, prompt, verdict, settles]);

    let sources = first_sources(&map);
    assert_eq!(sources[0]["content"].as_str().unwrap().len(), 4000);
    assert_eq!(sources[0]["truncated"], true);
}
