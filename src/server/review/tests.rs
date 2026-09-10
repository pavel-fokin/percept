use serde_json::Value;

use super::*;
use crate::core::testing::{
    edge_added, human, node_added_by, node_id, schemas, source, FakeLog,
};
use crate::core::{Actor, Event, NodeId, Payload};
use crate::shared::Timestamp;

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
