use std::path::PathBuf;

use serde_json::Value;

use super::*;
use crate::core::testing::{
    created_at, edge_added, human, node_added_by, node_added_citing, schemas, source, source_at,
    FakeLog,
};
use crate::core::{Actor, Event, EventId, Payload};
use crate::shared::Timestamp;

fn message_received(actor: Actor, content: &str) -> Event {
    Event::new(actor, source("test"), None, Payload::MessageReceived { content: content.to_string() })
}

fn message_received_at(source: Source, actor: Actor, content: &str) -> Event {
    Event::new(actor, source, None, Payload::MessageReceived { content: content.to_string() })
}

fn file_cited(path: &str, lines: Option<(u32, u32)>, excerpt: &str) -> Event {
    Event::new(
        Actor::Agent,
        source("test"),
        None,
        Payload::FileCited {
            path: PathBuf::from(path),
            lines,
            excerpt: excerpt.to_string(),
        },
    )
}

fn cut_body_since(events: Vec<Event>, since: Option<Timestamp>) -> Value {
    let log = FakeLog::seeded(events);
    let schemas = schemas();
    let src = source("test");
    serde_json::to_value(cut(&log, &schemas, &src, since).unwrap()).unwrap()
}

fn cut_decisions(events: Vec<Event>) -> Value {
    cut_decisions_since(events, None)
}

fn cut_decisions_since(events: Vec<Event>, since: Option<Timestamp>) -> Value {
    let body = cut_body_since(events, since);
    body["maps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|map| map["name"] == "decisions")
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
fn a_claimed_decision_appears_under_the_question_it_resolves() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, decision, resolves]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["heading"]["title"], "Which language?");
    let claims = claims_of(&map, 0);
    assert_eq!(claims.len(), 2, "the question is a row of its own group too");
    assert_eq!(claims[0]["name"], "Which language?");
    assert_eq!(claims[1]["name"], "Rust");
}

#[test]
fn cut_with_a_since_after_a_nodes_changed_at_leaves_it_out() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let t0 = Timestamp::now();
    let seeded = created_at(decision, t0);
    let since = t0.minus_minutes(-10).unwrap();

    let map = cut_decisions_since(vec![seeded], Some(since));

    assert!(groups_of(&map).is_empty(), "{map:?}");
}

#[test]
fn cut_with_no_since_includes_every_headline_node() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");

    let map = cut_decisions(vec![decision]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(claims_of(&map, 0)[0]["name"], "Rust");
}

#[test]
fn a_since_at_or_before_a_nodes_changed_at_keeps_it_in_the_cut() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let t0 = Timestamp::now();
    let seeded = created_at(decision, t0);

    let map = cut_decisions_since(vec![seeded], Some(t0));

    assert_eq!(claims_of(&map, 0)[0]["name"], "Rust");
}

#[test]
fn a_reopening_question_groups_under_the_decision_it_doubts() {
    // No edge kind sorts a group ahead of another: groups order by
    // their heading's `added_at`, so the older, unrelated question
    // still comes first.
    let older_question = node_added_by(Actor::Agent, "question", "Older question?");
    let decision = node_added_by(Actor::Agent, "decision", "Something");
    let newer_question = node_added_by(Actor::Agent, "question", "Newer, reopening?");
    let reopens = edge_added("reopens", &newer_question, &decision);

    let map = cut_decisions(vec![older_question, decision, newer_question, reopens]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 2);
    assert_eq!(claims_of(&map, 0)[0]["name"], "Older question?");
    assert_eq!(groups[1]["heading"]["title"], "Something");
    assert_eq!(claims_of(&map, 1)[1]["name"], "Newer, reopening?");
}

#[test]
fn an_open_question_with_no_decision_is_its_own_group_with_a_null_heading() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");

    let map = cut_decisions(vec![question]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["heading"].is_null(), "{:?}", groups[0]);
    let claims = claims_of(&map, 0);
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["name"], "Which language?");
}

#[test]
fn a_decision_that_supersedes_another_groups_under_that_one_not_its_question() {
    // The heading search is one hop, checking edge kinds in schema
    // order: `correction` has no `resolves` edge of its own, so it
    // groups under `old_decision`, the node its `supersedes` edge
    // reaches - the core keeps no supersession chain for the review to
    // follow further.
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let old_decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &old_decision, &question);
    let correction = node_added_by(Actor::Agent, "decision", "Go");
    let supersedes = edge_added("supersedes", &correction, &old_decision);

    let map = cut_decisions(vec![question, old_decision, resolves, correction, supersedes]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["heading"]["title"], "Which language?");
    assert_eq!(claims_of(&map, 0)[1]["name"], "Rust");
    assert_eq!(groups[1]["heading"]["title"], "Rust");
    assert_eq!(claims_of(&map, 1)[0]["name"], "Go");
}

#[test]
fn an_orphan_group_carries_a_null_heading() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");

    let map = cut_decisions(vec![decision]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert!(groups[0]["heading"].is_null(), "{:?}", groups[0]);
}

#[test]
fn an_option_that_answers_the_question_is_related_under_the_decisions_row() {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);
    let option = node_added_by(Actor::Agent, "option", "Go");
    let answers = edge_added("answers", &option, &question);

    let map = cut_decisions(vec![question, decision, resolves, option, answers]);

    let claims = claims_of(&map, 0);
    let options = claims[0]["related"].as_array().unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0]["name"], "Go");
    assert_eq!(options[0]["changed_by"], "agent");
}

#[test]
fn a_decision_citing_a_human_prompt_carries_the_prompts_content_and_the_agent_reply_before_it_as_its_proposal(
) {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let t0 = Timestamp::now();
    let proposal = created_at(message_received(Actor::Agent, "Recommendation: Rust."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Yes, Rust."), t1);
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, proposal, prompt, decision, resolves]);

    let sources = first_sources(&map);
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["kind"], "message");
    assert_eq!(sources[0]["content"], "Yes, Rust.");
    assert_eq!(sources[0]["proposal"]["content"], "Recommendation: Rust.");
}

#[test]
fn a_prompt_with_no_earlier_agent_reply_in_its_source_carries_a_null_proposal() {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let prompt = message_received(Actor::Human(human()), "Rust, please.");
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, prompt, decision, resolves]);

    let sources = first_sources(&map);
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_reply_from_another_source_is_not_taken_as_the_proposal() {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let t0 = Timestamp::now();
    let other_reply = created_at(message_received_at(source_at("other", "/other"), Actor::Agent, "Go."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Rust, please."), t1);
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, other_reply, prompt, decision, resolves]);

    let sources = first_sources(&map);
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_file_cited_source_carries_its_path_lines_and_excerpt() {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let cite = file_cited("src/main.rs", Some((10, 20)), "fn main() {}");
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![cite.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, cite, decision, resolves]);

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
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let missing_id = EventId::new();
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![missing_id]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, decision, resolves]);

    let sources = first_sources(&map);
    assert_eq!(sources[0]["kind"], "missing");
}

#[test]
fn content_over_4000_characters_is_cut_and_marked_truncated() {
    let question = node_added_by(Actor::Human(None), "question", "Which language?");
    let long = "a".repeat(4001);
    let prompt = message_received(Actor::Human(human()), &long);
    let decision = node_added_citing(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, prompt, decision, resolves]);

    let sources = first_sources(&map);
    assert_eq!(sources[0]["content"].as_str().unwrap().len(), 4000);
    assert_eq!(sources[0]["truncated"], true);
}
