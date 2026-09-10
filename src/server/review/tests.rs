use std::collections::BTreeMap;
use std::path::PathBuf;

use serde_json::Value;

use super::*;
use crate::core::testing::{
    edge_added, human, node_added_by, node_id, schemas, source, source_at, FakeLog, ROOT,
};
use crate::core::{Actor, Event, EventId, NodeId, Payload};
use crate::shared::Timestamp;

/// A `node.added` event on the decisions map, citing `sources` - the
/// one way a test points a row at events of its own choosing, since
/// `core::testing::node_added_by` mints a source id no event answers
/// to.
fn node_added_with_sources(actor: Actor, kind: &str, name: &str, sources: Vec<EventId>) -> Event {
    Event::new(
        actor,
        source("test"),
        None,
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: kind.to_string(),
            name: name.to_string(),
            properties: BTreeMap::new(),
            sources,
            seq: 0,
        },
    )
}

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

fn review_finished(map: &str, nodes: Vec<NodeId>) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        Payload::ReviewFinished {
            map: map.to_string(),
            nodes,
        },
    )
}

fn claim_disputed(node: NodeId, why: &str) -> Event {
    Event::new(
        Actor::Human(human()),
        source("test"),
        None,
        Payload::ClaimDisputed {
            map: "decisions".to_string(),
            node,
            why: why.to_string(),
        },
    )
}

/// `event`, restamped to `at` - the one way a test controls the order
/// `Map::judged_since` and this cut's grouping see, mirroring
/// `core::map::tests::created_at`.
fn created_at(event: Event, at: Timestamp) -> Event {
    Event::restore(
        event.id(),
        event.actor(),
        event.source().clone(),
        event.causation_id(),
        at,
        event.payload().clone(),
    )
}

fn cut_decisions(events: Vec<Event>) -> Value {
    let log = FakeLog::seeded(events);
    let schemas = schemas();
    let src = source("test");
    let body = cut(&log, &schemas, &src).unwrap();
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

#[test]
fn a_claimed_decision_appears_under_the_question_it_resolves() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, decision, resolves]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["title"], "Which language?");
    let claims = groups[0]["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["name"], "Rust");
    assert_eq!(claims[0]["standing"], "claimed");
}

#[test]
fn a_decision_a_review_has_finished_is_not_in_the_cut() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);
    let finished = review_finished("decisions", vec![node_id(&decision)]);

    let map = cut_decisions(vec![question, decision, resolves, finished]);

    let seen_anywhere = groups_of(&map).iter().any(|group| {
        group["claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|claim| claim["name"] == "Rust")
    });
    assert!(!seen_anywhere, "{map:?}");
}

#[test]
fn a_decision_disputed_after_the_last_finish_stays_in_the_cut_with_its_why() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);
    let t1 = Timestamp::now();
    let finished = created_at(review_finished("decisions", vec![node_id(&decision)]), t1);
    let t2 = t1.minus_minutes(-10).unwrap();
    let disputed = created_at(claim_disputed(node_id(&decision), "never proposed"), t2);

    let map = cut_decisions(vec![question, decision, resolves, finished, disputed]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    let claims = groups[0]["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["name"], "Rust");
    assert_eq!(claims[0]["standing"], "disputed");
    assert_eq!(claims[0]["dispute"], "never proposed");
}

#[test]
fn a_question_that_reopens_a_decision_sorts_before_an_older_question() {
    let older_question = node_added_by(Actor::Agent, "question", "Older question?");
    let decision = node_added_by(Actor::Agent, "decision", "Something");
    let newer_question = node_added_by(Actor::Agent, "question", "Newer, reopening?");
    let reopens = edge_added("reopens", &newer_question, &decision);

    let map = cut_decisions(vec![older_question, decision, newer_question, reopens]);

    let groups = groups_of(&map);
    assert_eq!(groups[0]["title"], "Newer, reopening?");
}

#[test]
fn an_open_question_with_no_decision_is_its_own_group_with_itself_as_the_only_row() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");

    let map = cut_decisions(vec![question]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["title"], "Which language?");
    let claims = groups[0]["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["id"], groups[0]["id"]);
    assert_eq!(claims[0]["name"], "Which language?");
}

fn cut_body(events: Vec<Event>) -> Value {
    let log = FakeLog::seeded(events);
    let schemas = schemas();
    let src = source("test");
    cut(&log, &schemas, &src).unwrap()
}

#[test]
fn next_carries_a_disputed_nodes_line_with_its_why_once_a_session_started_precedes_the_dispute() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let t0 = Timestamp::now();
    let started = created_at(Event::session_started(source("test")), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let disputed = created_at(claim_disputed(node_id(&decision), "nah"), t1);

    let body = cut_body(vec![decision, started, disputed]);

    let next = body["next"].as_str().expect("next carries the judged block");
    assert!(next.contains("d1"), "{next}");
    assert!(next.contains("nah"), "{next}");
}

#[test]
fn next_is_null_when_nothing_was_judged() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let started = Event::session_started(source("test"));

    let body = cut_body(vec![decision, started]);

    assert!(body["next"].is_null(), "{body}");
}

#[test]
fn a_decision_that_supersedes_the_one_resolving_a_question_is_grouped_under_that_question() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let old_decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &old_decision, &question);
    let correction = node_added_by(Actor::Agent, "decision", "Go");
    let supersedes = edge_added("supersedes", &correction, &old_decision);

    let map = cut_decisions(vec![question, old_decision, resolves, correction, supersedes]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["title"], "Which language?");
    let claims = groups[0]["claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["name"], "Go");
}

#[test]
fn an_orphan_group_carries_a_null_raised_at() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");

    let map = cut_decisions(vec![decision]);

    let groups = groups_of(&map);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["id"], "");
    assert!(groups[0]["raised_at"].is_null(), "{:?}", groups[0]);
}

#[test]
fn an_option_whose_name_equals_the_decisions_is_not_listed_under_it() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);
    let restating_option = node_added_by(Actor::Agent, "option", "Rust");
    let answers = edge_added("answers", &restating_option, &question);

    let map = cut_decisions(vec![question, decision, resolves, restating_option, answers]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let options = claims[0]["options"].as_array().unwrap();
    assert!(options.is_empty(), "{options:?}");
}

#[test]
fn an_option_that_answers_the_question_is_listed_under_the_decisions_row() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let resolves = edge_added("resolves", &decision, &question);
    let option = node_added_by(Actor::Agent, "option", "Go");
    let answers = edge_added("answers", &option, &question);

    let map = cut_decisions(vec![question, decision, resolves, option, answers]);

    let groups = groups_of(&map);
    let claims = groups[0]["claims"].as_array().unwrap();
    let options = claims[0]["options"].as_array().unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0]["name"], "Go");
    assert_eq!(options[0]["standing"], "claimed");
}

#[test]
fn a_decision_citing_a_human_prompt_carries_the_prompts_content_and_the_agent_reply_before_it_as_its_proposal(
) {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let t0 = Timestamp::now();
    let proposal = created_at(message_received(Actor::Agent, "Recommendation: Rust."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Yes, Rust."), t1);
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, proposal, prompt, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0]["kind"], "message");
    assert_eq!(sources[0]["content"], "Yes, Rust.");
    assert_eq!(sources[0]["proposal"]["content"], "Recommendation: Rust.");
}

#[test]
fn a_prompt_with_no_earlier_agent_reply_in_its_source_carries_a_null_proposal() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let prompt = message_received(Actor::Human(human()), "Rust, please.");
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, prompt, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_reply_from_another_source_is_not_taken_as_the_proposal() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let t0 = Timestamp::now();
    let other_reply = created_at(message_received_at(source_at("other", "/other"), Actor::Agent, "Go."), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let prompt = created_at(message_received(Actor::Human(human()), "Rust, please."), t1);
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, other_reply, prompt, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert!(sources[0]["proposal"].is_null(), "{sources:?}");
}

#[test]
fn a_file_cited_source_carries_its_path_lines_and_excerpt() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let cite = file_cited("src/main.rs", Some((10, 20)), "fn main() {}");
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![cite.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, cite, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert_eq!(sources[0]["kind"], "file");
    assert_eq!(sources[0]["path"], "src/main.rs");
    assert_eq!(sources[0]["lines"][0], 10);
    assert_eq!(sources[0]["lines"][1], 20);
    assert_eq!(sources[0]["excerpt"], "fn main() {}");
}

#[test]
fn a_source_id_the_log_does_not_hold_reads_as_missing() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let missing_id = EventId::new();
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![missing_id]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert_eq!(sources[0]["kind"], "missing");
}

#[test]
fn a_finish_naming_an_unknown_id_still_appends_a_review_finished_naming_the_ones_that_resolved() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let log = FakeLog::seeded(vec![decision.clone()]);
    let schemas = schemas();
    let src = source("test");

    let outcome = finish(&log, &schemas, &src, human(), "decisions", &["d1".to_string(), "d99".to_string()]);

    assert!(matches!(outcome, ApiOutcome::Ok(_)));
    let events = log.load().unwrap();
    let finished = events
        .iter()
        .find_map(|event| match event.payload() {
            Payload::ReviewFinished { nodes, .. } => Some(nodes.clone()),
            _ => None,
        })
        .expect("a review.finished event");
    assert_eq!(finished, vec![node_id(&decision)]);
}

#[test]
fn a_judgment_between_two_clients_last_sessions_is_in_next() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let t0 = Timestamp::now();
    let earlier_session = created_at(Event::session_started(source_at("claude-code", ROOT)), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let disputed = created_at(claim_disputed(node_id(&decision), "nah"), t1);
    let t2 = t1.minus_minutes(-10).unwrap();
    let later_session = created_at(Event::session_started(source_at("codex", ROOT)), t2);

    let body = cut_body(vec![decision, earlier_session, disputed, later_session]);

    let next = body["next"].as_str().expect("next carries the judged block");
    assert!(next.contains("nah"), "{next}");
}

#[test]
fn a_judgment_with_no_session_at_all_is_in_next() {
    let decision = node_added_by(Actor::Agent, "decision", "Rust");
    let t0 = Timestamp::now();
    let seeded_decision = created_at(decision.clone(), t0);
    let t1 = t0.minus_minutes(-10).unwrap();
    let disputed = created_at(claim_disputed(node_id(&decision), "nah"), t1);

    let body = cut_body(vec![seeded_decision, disputed]);

    let next = body["next"].as_str().expect("next carries the judged block");
    assert!(next.contains("nah"), "{next}");
}

#[test]
fn content_over_4000_characters_is_cut_and_marked_truncated() {
    let question = node_added_by(Actor::Agent, "question", "Which language?");
    let long = "a".repeat(4001);
    let prompt = message_received(Actor::Human(human()), &long);
    let decision = node_added_with_sources(Actor::Agent, "decision", "Rust", vec![prompt.id()]);
    let resolves = edge_added("resolves", &decision, &question);

    let map = cut_decisions(vec![question, prompt, decision, resolves]);

    let claims = groups_of(&map)[0]["claims"].as_array().unwrap();
    let sources = claims[0]["sources"].as_array().unwrap();
    assert_eq!(sources[0]["content"].as_str().unwrap().len(), 4000);
    assert_eq!(sources[0]["truncated"], true);
}
