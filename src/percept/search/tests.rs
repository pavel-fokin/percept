use super::*;
use std::collections::BTreeMap;

use crate::percept::{EventId, NodeId, Payload};
use crate::testing::source;

/// A message from `name`, timestamped `offset_minutes` back.
fn event_at(name: &str, offset_minutes: i64) -> Event {
    Event::restore(
        EventId::new(),
        Actor::User,
        source(name),
        None,
        Timestamp::now().minus_minutes(offset_minutes).unwrap(),
        Payload::MessageReceived {
            content: "hi".to_string(),
        },
    )
}

fn sources(events: &[Event]) -> Vec<String> {
    events.iter().map(|e| e.source().name.clone()).collect()
}

/// A user message from `tui` carrying `content`, for the
/// text-filter tests.
fn message(content: &str) -> Event {
    Event::restore(
        EventId::new(),
        Actor::User,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::MessageReceived {
            content: content.to_string(),
        },
    )
}

fn contents(events: &[Event]) -> Vec<String> {
    events
        .iter()
        .map(|e| match e.payload() {
            Payload::MessageReceived { content } => content.clone(),
            _ => panic!("not a message"),
        })
        .collect()
}

#[test]
fn a_default_query_keeps_everything_in_the_order_it_was_given() {
    let events = vec![event_at("a", 2), event_at("b", 1)];
    let kept = EventQuery::default().apply(events);
    assert_eq!(sources(&kept), vec!["a", "b"]);
}

#[test]
fn size_keeps_the_most_recent_matches_but_preserves_log_order() {
    let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

    let kept = EventQuery {
        size: Some(2),
        ..Default::default()
    }
    .apply(events);

    assert_eq!(sources(&kept), vec!["b", "c"]);
}

#[test]
fn size_larger_than_the_log_keeps_everything() {
    let events = vec![event_at("a", 2), event_at("b", 1)];

    let kept = EventQuery {
        size: Some(10),
        ..Default::default()
    }
    .apply(events);

    assert_eq!(kept.len(), 2);
}

#[test]
fn since_is_inclusive_and_until_is_exclusive() {
    let a = event_at("a", 30);
    let b = event_at("b", 20);
    let c = event_at("c", 10);

    let kept = EventQuery {
        since: Some(b.created_at()),
        until: Some(c.created_at()),
        ..Default::default()
    }
    .apply(vec![a, b, c]);

    assert_eq!(sources(&kept), vec!["b"]);
}

#[test]
fn a_multi_valued_filter_matches_any_of_its_values() {
    let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

    let kept = EventQuery {
        sources: vec!["a".to_string(), "c".to_string()],
        ..Default::default()
    }
    .apply(events);

    assert_eq!(sources(&kept), vec!["a", "c"]);
}

#[test]
fn filters_are_anded_together() {
    let mut wanted = event_at("a", 2);
    wanted = Event::restore(
        wanted.id(),
        Actor::Model,
        source("a"),
        None,
        wanted.created_at(),
        Payload::MessageReceived {
            content: "hi".to_string(),
        },
    );

    let query = EventQuery {
        sources: vec!["a".to_string()],
        actors: vec![Actor::Model],
        ..Default::default()
    };

    assert!(query.matches(&wanted));
    assert!(!query.matches(&event_at("a", 1)));
}

#[test]
fn a_text_term_matches_a_substring_case_insensitively() {
    let events = vec![message("Deploy the API"), message("hello")];

    let kept = EventQuery {
        text: vec!["deploy".to_string()],
        ..Default::default()
    }
    .apply(events);

    assert_eq!(contents(&kept), vec!["Deploy the API"]);
}

#[test]
fn a_hit_is_the_earliest_offset_of_any_term_in_content() {
    let event = message("Ship it, then DEPLOY it, then ship again");
    let query = EventQuery {
        text: vec!["deploy".to_string(), "then".to_string()],
        ..Default::default()
    };
    assert_eq!(query.hit(&event), Some(9..13));

    let off = EventQuery::default();
    assert_eq!(off.hit(&event), None);
}

#[test]
fn a_hit_counts_characters_of_the_original_text() {
    // `İ` lowercases to two characters; an offset taken on the
    // lowercased copy would land one past the term.
    let event = message("İİ deploy");
    let query = EventQuery {
        text: vec!["deploy".to_string()],
        ..Default::default()
    };
    assert_eq!(query.hit(&event), Some(3..9));
}

#[test]
fn a_hit_spans_the_original_characters_of_a_term_that_expands() {
    let event = message("say İ now");
    let query = EventQuery {
        text: vec!["İ".to_string()],
        ..Default::default()
    };
    assert_eq!(query.hit(&event), Some(4..5));
}

#[test]
fn a_final_sigma_matches_and_hits_alike() {
    let event = message("ΟΔΥΣΣΕΥΣ went home");
    let query = EventQuery {
        text: vec!["ΟΔΥΣΣΕΥΣ".to_string()],
        ..Default::default()
    };
    assert!(query.matches(&event));
    assert_eq!(query.hit(&event), Some(0..8));
}

#[test]
fn an_empty_term_matches_empty_content_without_panicking() {
    let event = message("");
    let query = EventQuery {
        text: vec![String::new()],
        ..Default::default()
    };
    assert!(query.matches(&event));
    assert_eq!(query.hit(&event), Some(0..0));
}

#[test]
fn a_tool_call_matching_on_its_tool_name_has_no_hit() {
    let call = Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "search_events".to_string(),
            arguments: "{}".to_string(),
        },
    );
    let query = EventQuery {
        text: vec!["search".to_string()],
        ..Default::default()
    };
    assert!(query.matches(&call));
    assert_eq!(query.hit(&call), None);
}

#[test]
fn a_text_term_matches_every_payload_kind() {
    let payloads = vec![
        Payload::MessageReceived {
            content: "deploy it".to_string(),
        },
        Payload::ThoughtRecorded {
            content: "deploy it".to_string(),
        },
        Payload::ToolResulted {
            content: "deploy it".to_string(),
        },
        Payload::ToolCalled {
            tool: "deploy_tool".to_string(),
            arguments: "{}".to_string(),
        },
        Payload::NodeAdded {
            map: "tasks".to_string(),
            node: NodeId::new(),
            kind: "goal".to_string(),
            name: "Deploy by Friday".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
        Payload::NodeAdded {
            map: "tasks".to_string(),
            node: NodeId::new(),
            kind: "goal".to_string(),
            name: "Ship".to_string(),
            properties: BTreeMap::from([("note".to_string(), "deploy first".to_string())]),
            sources: Vec::new(),
        },
        Payload::NodeRemoved {
            map: "tasks".to_string(),
            node: NodeId::new(),
            reason: "deployed already".to_string(),
            sources: Vec::new(),
        },
        Payload::EdgeAdded {
            map: "deploys".to_string(),
            kind: "blocks".to_string(),
            from: NodeId::new(),
            to: NodeId::new(),
            sources: Vec::new(),
        },
        Payload::EdgeRemoved {
            map: "tasks".to_string(),
            kind: "deploys_to".to_string(),
            from: NodeId::new(),
            to: NodeId::new(),
            sources: Vec::new(),
        },
    ];
    let query = EventQuery {
        text: vec!["deploy".to_string()],
        ..Default::default()
    };

    for payload in payloads {
        let event = Event::restore(
            EventId::new(),
            Actor::User,
            source("tui"),
            None,
            Timestamp::now(),
            payload,
        );
        assert!(query.matches(&event));
    }
}

#[test]
fn a_tool_call_matches_by_tool_name_or_by_arguments() {
    let call = Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "search_events".to_string(),
            arguments: r#"{"kinds":["tool.called"]}"#.to_string(),
        },
    );

    let by_tool = EventQuery {
        text: vec!["search".to_string()],
        ..Default::default()
    };
    let by_arguments = EventQuery {
        text: vec!["tool.called".to_string()],
        ..Default::default()
    };

    assert!(by_tool.matches(&call));
    assert!(by_arguments.matches(&call));
}

#[test]
fn a_text_term_does_not_match_the_envelope() {
    // The source is "claude-code"; the payload only says "hi".
    let event = event_at("claude-code", 1);

    let query = EventQuery {
        text: vec!["claude".to_string()],
        ..Default::default()
    };

    assert!(!query.matches(&event));
}

#[test]
fn text_terms_match_any_of_their_values() {
    let events = vec![
        message("deploy the api"),
        message("ship it"),
        message("hello"),
    ];

    let kept = EventQuery {
        text: vec!["deploy".to_string(), "SHIP".to_string()],
        ..Default::default()
    }
    .apply(events);

    assert_eq!(contents(&kept), vec!["deploy the api", "ship it"]);
}

#[test]
fn a_blank_term_matches_everything_as_documented() {
    let events = vec![message("a"), message("b")];

    let kept = EventQuery {
        text: vec![String::new()],
        ..Default::default()
    }
    .apply(events);

    assert_eq!(kept.len(), 2);
}
