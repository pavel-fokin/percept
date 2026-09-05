use super::*;
use crate::testing::{source, usage};

#[test]
fn a_short_payload_is_left_alone() {
    let payload = serde_json::json!({"content": "hi"});
    assert_eq!(shorten(payload.clone()), payload);
}

#[test]
fn a_long_string_is_cut_but_its_object_keeps_its_shape() {
    let payload = serde_json::json!({
        "tool_name": "Edit",
        "tool_result": "a".repeat(500),
    });
    let short = shorten(payload);

    // Still an object, so one jq expression reads this and the
    // whole payload alike.
    assert_eq!(short["tool_name"], "Edit");
    let cut = short["tool_result"].as_str().unwrap();
    assert!(cut.ends_with('\u{2026}'));
    assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
}

#[test]
fn a_cut_never_splits_a_multi_byte_character() {
    // 119 ascii chars, then a 3-byte character straddling the cut.
    // Truncating by bytes would split it.
    let payload = serde_json::json!({ "c": format!("{}\u{20ac}\u{20ac}", "a".repeat(119)) });
    let short = shorten(payload);

    let cut = short["c"].as_str().unwrap();
    assert!(std::str::from_utf8(cut.as_bytes()).is_ok());
    assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
}

#[test]
fn a_summary_reports_the_length_of_a_cut_content_and_nothing_else() {
    let long = message(Actor::Model, "x".repeat(500));
    let line: Value = serde_json::from_str(&summarize(&long, None, PREVIEW_CHARS)).unwrap();
    assert_eq!(line["preview"]["len"], 500);
    assert_eq!(
        line["payload"]["content"].as_str().unwrap().chars().count(),
        PREVIEW_CHARS + 1
    );

    let short = message(Actor::Model, "hi".to_string());
    let line: Value = serde_json::from_str(&summarize(&short, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
}

#[test]
fn a_hit_deep_in_content_sits_inside_its_preview() {
    let text = format!("{}deploy{}", "a".repeat(400), "b".repeat(400));
    let event = message(Actor::Model, text);
    let line: Value =
        serde_json::from_str(&summarize(&event, Some(400..406), PREVIEW_CHARS)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.contains("deploy"));
    assert!(cut.starts_with('\u{2026}') && cut.ends_with('\u{2026}'));
    assert_eq!(cut.chars().count(), PREVIEW_CHARS + 2);
    assert_eq!(line["preview"]["match"], 400);
}

#[test]
fn a_term_wider_than_half_the_window_still_fits_in_it() {
    let text = format!("{}deployment pipeline{}", "a".repeat(400), "b".repeat(400));
    let event = message(Actor::Model, text);
    let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 20)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.contains("deployment pipeline"), "{cut}");

    let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 10)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.contains("deployment"), "{cut}");
}

#[test]
fn a_preview_without_a_hit_carries_no_match() {
    let event = message(Actor::Model, "x".repeat(500));
    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert!(line["preview"].get("match").is_none());
}

#[test]
fn a_hit_near_the_end_pulls_the_window_back_rather_than_past_it() {
    let text = format!("{}deploy", "a".repeat(400));
    let event = message(Actor::Model, text);
    let line: Value =
        serde_json::from_str(&summarize(&event, Some(400..406), PREVIEW_CHARS)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.starts_with('\u{2026}') && cut.ends_with("deploy"));
    assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
}

#[test]
fn the_preview_window_is_the_callers_size() {
    let event = message(Actor::Model, "x".repeat(500));
    let line: Value = serde_json::from_str(&summarize(&event, None, 10)).unwrap();
    assert_eq!(
        line["payload"]["content"].as_str().unwrap().chars().count(),
        11
    );

    let line: Value = serde_json::from_str(&summarize(&event, None, 1000)).unwrap();
    assert_eq!(
        line["payload"]["content"].as_str().unwrap().chars().count(),
        500
    );
    assert!(line.get("preview").is_none());
}

#[test]
fn a_cut_inside_arguments_is_not_a_preview() {
    let call = percept::Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "search_events".to_string(),
            arguments: format!(r#"{{"contains":["{}"]}}"#, "y".repeat(500)),
        },
    );
    let line: Value = serde_json::from_str(&summarize(&call, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
    assert!(line["payload"]["arguments"]["contains"][0]
        .as_str()
        .unwrap()
        .ends_with('\u{2026}'));
}

#[test]
fn excerpt_slices_content_and_reports_the_whole_length() {
    let event = message(Actor::Model, "hello world".to_string());
    let line = excerpt(&event, Some(0), Some(5)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hello");
    assert_eq!(value["preview"]["len"], 11);
}

#[test]
fn excerpt_never_splits_a_multi_byte_character() {
    // "aaa" then two 3-byte euro signs - a byte slice at 4 would
    // split the first one.
    let event = message(Actor::Model, format!("aaa{}", "\u{20ac}\u{20ac}"));
    let line = excerpt(&event, Some(3), Some(4)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "\u{20ac}");
}

#[test]
fn excerpt_defaults_start_to_zero_and_end_to_the_length() {
    let event = message(Actor::Model, "hi".to_string());
    let line = excerpt(&event, None, None).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hi");
    assert_eq!(value["preview"]["len"], 2);
}

#[test]
fn excerpt_clamps_an_end_past_the_length() {
    let event = message(Actor::Model, "hi".to_string());
    let line = excerpt(&event, Some(0), Some(9000)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hi");
}

#[test]
fn excerpt_rejects_a_start_past_the_length_and_names_it() {
    let event = message(Actor::Model, "hi".to_string());
    let err = excerpt(&event, Some(9000), None).unwrap_err().to_string();
    assert_eq!(err, "start 9000 is past the end of content (2 characters)");
}

#[test]
fn excerpt_rejects_an_inverted_range_and_names_both_ends() {
    let event = message(Actor::Model, "hello".to_string());
    let err = excerpt(&event, Some(4), Some(2)).unwrap_err().to_string();
    assert_eq!(err, "start 4 is not before end 2");
}

#[test]
fn excerpt_on_a_tool_called_event_is_an_error() {
    let call = percept::Event::restore(
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
    let err = excerpt(&call, None, None).unwrap_err().to_string();
    assert_eq!(err, "tool.called has no content to slice");
}

fn message(actor: Actor, content: String) -> percept::Event {
    percept::Event::message_received(actor, content, source("tui"), None)
}

#[test]
fn nested_and_array_values_are_reached() {
    let payload = serde_json::json!({"a": {"b": ["x".repeat(500)]}});
    let short = shorten(payload);

    assert_eq!(
        short["a"]["b"][0].as_str().unwrap().chars().count(),
        PREVIEW_CHARS + 1
    );
}

#[test]
fn round_trips_through_json() {
    let cause = EventId::new();
    let original = percept::Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        Some(cause),
        Timestamp::now(),
        Payload::MessageReceived {
            content: "hello world".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    let restored = percept::Event::try_from(wire).unwrap();

    assert!(restored.id() == original.id());
    assert_eq!(restored.source(), original.source());
    assert!(restored.actor() == original.actor());
    assert!(restored.causation_id() == original.causation_id());
    assert!(restored.created_at() == original.created_at());
    match restored.payload() {
        Payload::MessageReceived { content } => assert_eq!(content, "hello world"),
        _ => panic!("expected MessageReceived"),
    }
}

#[test]
fn thought_recorded_round_trips_through_json() {
    let original = percept::Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ThoughtRecorded {
            content: "let me think".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "thought.recorded");
    let restored = percept::Event::try_from(wire).unwrap();

    match restored.payload() {
        Payload::ThoughtRecorded { content } => assert_eq!(content, "let me think"),
        _ => panic!("expected ThoughtRecorded"),
    }
}

#[test]
fn tool_called_round_trips_with_arguments_as_a_nested_object() {
    let original = percept::Event::restore(
        EventId::new(),
        Actor::Model,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "search_events".to_string(),
            arguments: r#"{"sources":["tui"],"size":5}"#.to_string(),
        },
    );

    let wire = Event::from(&original);
    assert_eq!(wire.kind, "tool.called");
    // `arguments` is a real object on the wire, indexable by jq.
    assert_eq!(wire.payload["arguments"]["size"], 5);

    let json = serde_json::to_string(&wire).unwrap();
    let reparsed: Event = serde_json::from_str(&json).unwrap();
    let restored = percept::Event::try_from(reparsed).unwrap();

    match restored.payload() {
        Payload::ToolCalled { tool, arguments } => {
            assert_eq!(tool, "search_events");
            let value: Value = serde_json::from_str(arguments).unwrap();
            assert_eq!(value["size"], 5);
        }
        _ => panic!("expected ToolCalled"),
    }
}

#[test]
fn tool_resulted_round_trips_through_json() {
    let cause = EventId::new();
    let original = percept::Event::restore(
        EventId::new(),
        Actor::System,
        source("tui"),
        Some(cause),
        Timestamp::now(),
        Payload::ToolResulted {
            content: "3 events".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "tool.resulted");
    assert_eq!(wire.actor, "system");
    let restored = percept::Event::try_from(wire).unwrap();

    assert!(restored.actor() == Actor::System);
    assert!(restored.causation_id() == Some(cause));
    match restored.payload() {
        Payload::ToolResulted { content } => assert_eq!(content, "3 events"),
        _ => panic!("expected ToolResulted"),
    }
}

#[test]
fn model_called_round_trips_through_json() {
    let cause = EventId::new();
    let original = percept::Event::restore(
        EventId::new(),
        Actor::System,
        source("tui"),
        Some(cause),
        Timestamp::now(),
        Payload::ModelCalled(usage()),
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "model.called");
    assert_eq!(wire.actor, "system");
    // Unreported cached tokens are left off the wire, not written
    // as null.
    assert!(wire.payload.get("cached_tokens").is_none());
    let restored = percept::Event::try_from(wire).unwrap();

    assert!(restored.actor() == Actor::System);
    assert!(restored.causation_id() == Some(cause));
    match restored.payload() {
        Payload::ModelCalled(restored) => assert_eq!(restored, &usage()),
        _ => panic!("expected ModelCalled"),
    }
}

#[test]
fn node_added_round_trips_through_json() {
    let cited = EventId::new();
    let node = NodeId::new();
    let mut properties = BTreeMap::new();
    properties.insert(
        "summary".to_string(),
        "Same features on both stacks".to_string(),
    );
    let original = percept::Event::restore(
        EventId::new(),
        Actor::User,
        source("cli"),
        None,
        Timestamp::now(),
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node,
            kind: "evidence".to_string(),
            name: "Both built in parallel".to_string(),
            properties: properties.clone(),
            sources: vec![cited],
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "node.added");
    let restored = percept::Event::try_from(wire).unwrap();

    match restored.payload() {
        Payload::NodeAdded {
            map,
            node: restored_node,
            kind,
            name,
            properties: restored_properties,
            sources,
        } => {
            assert_eq!(map, "decisions");
            assert!(*restored_node == node);
            assert_eq!(kind, "evidence");
            assert_eq!(name, "Both built in parallel");
            assert_eq!(*restored_properties, properties);
            assert!(sources == &vec![cited]);
        }
        _ => panic!("expected NodeAdded"),
    }
}

#[test]
fn node_removed_round_trips_through_json() {
    let node = NodeId::new();
    let original = percept::Event::restore(
        EventId::new(),
        Actor::System,
        source("cli"),
        None,
        Timestamp::now(),
        Payload::NodeRemoved {
            map: "decisions".to_string(),
            node,
            reason: "superseded".to_string(),
            sources: Vec::new(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "node.removed");
    // Empty `sources` encodes as `[]`, never omitted.
    assert_eq!(wire.payload["sources"], serde_json::json!([]));
    let restored = percept::Event::try_from(wire).unwrap();

    match restored.payload() {
        Payload::NodeRemoved {
            map,
            node: restored_node,
            reason,
            sources,
        } => {
            assert_eq!(map, "decisions");
            assert!(*restored_node == node);
            assert_eq!(reason, "superseded");
            assert!(sources.is_empty());
        }
        _ => panic!("expected NodeRemoved"),
    }
}

#[test]
fn edge_added_round_trips_through_json() {
    let from = NodeId::new();
    let to = NodeId::new();
    let original = percept::Event::restore(
        EventId::new(),
        Actor::System,
        source("cli"),
        None,
        Timestamp::now(),
        Payload::EdgeAdded {
            map: "decisions".to_string(),
            kind: "supports".to_string(),
            from,
            to,
            sources: Vec::new(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "edge.added");
    let restored = percept::Event::try_from(wire).unwrap();

    match restored.payload() {
        Payload::EdgeAdded {
            map,
            kind,
            from: restored_from,
            to: restored_to,
            ..
        } => {
            assert_eq!(map, "decisions");
            assert_eq!(kind, "supports");
            assert!(*restored_from == from);
            assert!(*restored_to == to);
        }
        _ => panic!("expected EdgeAdded"),
    }
}

#[test]
fn a_malformed_source_in_a_node_added_payload_is_an_error() {
    let payload = serde_json::json!({
        "map": "decisions",
        "node": NodeId::new().as_uuid().to_string(),
        "kind": "evidence",
        "name": "x",
        "properties": {},
        "sources": ["not-a-uuid"],
    });

    let err = match decode("user", source("cli"), "node.added", None, payload) {
        Err(e) => e,
        Ok(_) => panic!("expected a malformed source to be rejected"),
    };
    assert!(matches!(err, Error::BadUuid(s) if s == "not-a-uuid"));
}

#[test]
fn a_map_events_summary_carries_no_preview() {
    let event = percept::Event::restore(
        EventId::new(),
        Actor::User,
        source("cli"),
        None,
        Timestamp::now(),
        Payload::NodeAdded {
            map: "decisions".to_string(),
            node: NodeId::new(),
            kind: "evidence".to_string(),
            name: "Both built in parallel".to_string(),
            properties: BTreeMap::new(),
            sources: Vec::new(),
        },
    );

    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
    assert_eq!(line["payload"]["name"], "Both built in parallel");
}

#[test]
fn a_model_called_summary_carries_no_preview() {
    let event = percept::Event::restore(
        EventId::new(),
        Actor::System,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ModelCalled(usage()),
    );

    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
    assert_eq!(line["payload"]["model"], "gpt-5");
    assert_eq!(line["payload"]["input_tokens"], 100);
}

#[test]
fn unknown_type_deserializes_but_has_no_domain_form() {
    let json = r#"{
        "id": "0192d1f0-1111-7000-8000-000000000000",
        "seq": 1,
        "actor": "user",
        "source": {"name": "tui", "path": "/test"},
        "type": "file.registered",
        "causation_id": null,
        "created_at": "2026-08-30T00:00:00Z",
        "payload": { "path": "/tmp/x" }
    }"#;

    let wire: Event = serde_json::from_str(json).expect("wire event deserializes");
    assert!(matches!(
        percept::Event::try_from(wire),
        Err(Error::UnknownEventType(_))
    ));
}

/// The wire format before `source` carried a path. There is no
/// migration: such a line is a bad line.
#[test]
fn a_bare_string_source_fails_to_deserialize() {
    let json = r#"{
        "id": "0192d1f0-1111-7000-8000-000000000000",
        "actor": "user",
        "source": "tui",
        "type": "message.received",
        "causation_id": null,
        "created_at": "2026-08-30T00:00:00Z",
        "payload": { "content": "hi" }
    }"#;

    assert!(serde_json::from_str::<Event>(json).is_err());
}
