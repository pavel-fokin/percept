use super::*;
use crate::core::testing::{human, source, usage};

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
    let long = message(Actor::Agent, "x".repeat(500));
    let line: Value = serde_json::from_str(&summarize(&long, None, PREVIEW_CHARS)).unwrap();
    assert_eq!(line["preview"]["len"], 500);
    assert_eq!(
        line["payload"]["content"].as_str().unwrap().chars().count(),
        PREVIEW_CHARS + 1
    );

    let short = message(Actor::Agent, "hi".to_string());
    let line: Value = serde_json::from_str(&summarize(&short, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
}

#[test]
fn a_hit_deep_in_content_sits_inside_its_preview() {
    let text = format!("{}deploy{}", "a".repeat(400), "b".repeat(400));
    let event = message(Actor::Agent, text);
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
    let event = message(Actor::Agent, text);
    let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 20)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.contains("deployment pipeline"), "{cut}");

    let line: Value = serde_json::from_str(&summarize(&event, Some(400..419), 10)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.contains("deployment"), "{cut}");
}

#[test]
fn a_preview_without_a_hit_carries_no_match() {
    let event = message(Actor::Agent, "x".repeat(500));
    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert!(line["preview"].get("match").is_none());
}

#[test]
fn a_hit_near_the_end_pulls_the_window_back_rather_than_past_it() {
    let text = format!("{}deploy", "a".repeat(400));
    let event = message(Actor::Agent, text);
    let line: Value =
        serde_json::from_str(&summarize(&event, Some(400..406), PREVIEW_CHARS)).unwrap();
    let cut = line["payload"]["content"].as_str().unwrap();
    assert!(cut.starts_with('\u{2026}') && cut.ends_with("deploy"));
    assert_eq!(cut.chars().count(), PREVIEW_CHARS + 1);
}

#[test]
fn the_preview_window_is_the_callers_size() {
    let event = message(Actor::Agent, "x".repeat(500));
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
    let call = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
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
    let event = message(Actor::Agent, "hello world".to_string());
    let line = excerpt(&event, Some(0), Some(5)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hello");
    assert_eq!(value["preview"]["len"], 11);
}

#[test]
fn excerpt_never_splits_a_multi_byte_character() {
    // "aaa" then two 3-byte euro signs - a byte slice at 4 would
    // split the first one.
    let event = message(Actor::Agent, format!("aaa{}", "\u{20ac}\u{20ac}"));
    let line = excerpt(&event, Some(3), Some(4)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "\u{20ac}");
}

#[test]
fn excerpt_defaults_start_to_zero_and_end_to_the_length() {
    let event = message(Actor::Agent, "hi".to_string());
    let line = excerpt(&event, None, None).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hi");
    assert_eq!(value["preview"]["len"], 2);
}

#[test]
fn excerpt_clamps_an_end_past_the_length() {
    let event = message(Actor::Agent, "hi".to_string());
    let line = excerpt(&event, Some(0), Some(9000)).unwrap();
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value["payload"]["content"], "hi");
}

#[test]
fn excerpt_rejects_a_start_past_the_length_and_names_it() {
    let event = message(Actor::Agent, "hi".to_string());
    let err = excerpt(&event, Some(9000), None).unwrap_err().to_string();
    assert_eq!(err, "start 9000 is past the end of content (2 characters)");
}

#[test]
fn excerpt_rejects_an_inverted_range_and_names_both_ends() {
    let event = message(Actor::Agent, "hello".to_string());
    let err = excerpt(&event, Some(4), Some(2)).unwrap_err().to_string();
    assert_eq!(err, "start 4 is not before end 2");
}

#[test]
fn excerpt_on_a_tool_called_event_is_an_error() {
    let call = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
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

fn message(actor: Actor, content: String) -> crate::core::Event {
    crate::core::Event::message_received(actor, content, source("tui"), None)
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
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("tui"),
        Some(cause),
        Timestamp::now(),
        Payload::MessageReceived {
            content: "hello world".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    let restored = crate::store::from_wire(wire).unwrap();

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
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
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
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::ThoughtRecorded { content } => assert_eq!(content, "let me think"),
        _ => panic!("expected ThoughtRecorded"),
    }
}

#[test]
fn tool_called_arguments_that_are_not_one_json_value_encode_as_a_string_not_a_panic() {
    let spliced = r#"{"path":"a"}{"path":"b"}"#;
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("tui"),
        None,
        Timestamp::now(),
        Payload::ToolCalled {
            tool: "read_file".to_string(),
            arguments: spliced.to_string(),
        },
    );

    let wire = Event::from(&original);
    assert_eq!(wire.payload["arguments"], spliced);

    let json = serde_json::to_string(&wire).unwrap();
    let restored =
        crate::store::from_wire(serde_json::from_str::<Event>(&json).unwrap()).unwrap();
    match restored.payload() {
        Payload::ToolCalled { arguments, .. } => {
            assert_eq!(serde_json::from_str::<Value>(arguments).unwrap(), spliced);
        }
        _ => panic!("expected ToolCalled"),
    }
}

#[test]
fn tool_called_round_trips_with_arguments_as_a_nested_object() {
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
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
    let restored = crate::store::from_wire(reparsed).unwrap();

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
    let original = crate::core::Event::restore(
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
    assert_eq!(wire.actor["kind"], "system");
    let restored = crate::store::from_wire(wire).unwrap();

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
    let original = crate::core::Event::restore(
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
    assert_eq!(wire.actor["kind"], "system");
    // Unreported cached tokens are left off the wire, not written
    // as null.
    assert!(wire.payload.get("cached_tokens").is_none());
    let restored = crate::store::from_wire(wire).unwrap();

    assert!(restored.actor() == Actor::System);
    assert!(restored.causation_id() == Some(cause));
    match restored.payload() {
        Payload::ModelCalled(restored) => assert_eq!(restored, &usage()),
        _ => panic!("expected ModelCalled"),
    }
}

#[test]
fn session_started_round_trips_through_json() {
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::System,
        source("claude-code"),
        None,
        Timestamp::now(),
        Payload::SessionStarted,
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "session.started");
    assert_eq!(wire.actor["kind"], "system");
    let restored = crate::store::from_wire(wire).unwrap();

    assert!(restored.actor() == Actor::System);
    assert!(matches!(restored.payload(), Payload::SessionStarted));
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
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(human()),
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
            seq: 3,
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "node.added");
    assert_eq!(wire.payload["seq"], 3);
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::NodeAdded {
            map,
            node: restored_node,
            kind,
            name,
            properties: restored_properties,
            sources,
            seq,
        } => {
            assert_eq!(map, "decisions");
            assert!(*restored_node == node);
            assert_eq!(kind, "evidence");
            assert_eq!(name, "Both built in parallel");
            assert_eq!(*restored_properties, properties);
            assert!(sources == &vec![cited]);
            assert_eq!(*seq, 3);
        }
        _ => panic!("expected NodeAdded"),
    }
}

#[test]
fn a_node_added_line_with_no_seq_decodes_to_the_sentinel() {
    let node = NodeId::new();
    let json = serde_json::json!({
        "map": "decisions",
        "node": node.as_uuid().to_string(),
        "kind": "evidence",
        "name": "x",
        "properties": {},
        "sources": [],
    });

    let event = decode("user", source("cli"), "node.added", None, json, human()).unwrap();

    assert!(matches!(event.payload(), Payload::NodeAdded { seq: 0, .. }));
}

#[test]
fn node_changed_round_trips_through_json() {
    let node = NodeId::new();
    let mut properties = BTreeMap::new();
    properties.insert("state".to_string(), "done".to_string());
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("cli"),
        None,
        Timestamp::now(),
        Payload::NodeChanged {
            map: "tasks".to_string(),
            node,
            name: Some("cancel a turn cleanly".to_string()),
            properties: properties.clone(),
            sources: Vec::new(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "node.changed");
    assert_eq!(wire.payload["name"], "cancel a turn cleanly");
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::NodeChanged {
            map,
            node: restored_node,
            name,
            properties: restored_properties,
            sources,
        } => {
            assert_eq!(map, "tasks");
            assert!(*restored_node == node);
            assert_eq!(name.as_deref(), Some("cancel a turn cleanly"));
            assert_eq!(*restored_properties, properties);
            assert!(sources.is_empty());
        }
        _ => panic!("expected NodeChanged"),
    }
}

#[test]
fn a_node_changed_line_with_no_name_decodes_to_none() {
    let node = NodeId::new();
    let json = serde_json::json!({
        "map": "tasks",
        "node": node.as_uuid().to_string(),
        "properties": {"state": "done"},
        "sources": [],
    });

    let event = decode("agent", source("cli"), "node.changed", None, json, human()).unwrap();

    assert!(matches!(event.payload(), Payload::NodeChanged { name: None, .. }));
}

#[test]
fn edge_added_round_trips_through_json() {
    let from = NodeId::new();
    let to = NodeId::new();
    let original = crate::core::Event::restore(
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
    let restored = crate::store::from_wire(wire).unwrap();

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

    let err = match decode("user", source("cli"), "node.added", None, payload, human()) {
        Err(e) => e,
        Ok(_) => panic!("expected a malformed source to be rejected"),
    };
    assert!(matches!(err, Error::BadUuid(s) if s == "not-a-uuid"));
}

#[test]
fn a_map_events_summary_carries_no_preview() {
    let event = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(human()),
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
            seq: 1,
        },
    );

    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert!(line.get("preview").is_none());
    assert_eq!(line["payload"]["name"], "Both built in parallel");
}

#[test]
fn a_model_called_summary_carries_no_preview() {
    let event = crate::core::Event::restore(
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
        "type": "file.moved",
        "causation_id": null,
        "created_at": "2026-08-30T00:00:00Z",
        "payload": { "path": "/tmp/x" }
    }"#;

    let wire: Event = serde_json::from_str(json).expect("wire event deserializes");
    assert!(matches!(
        crate::store::from_wire(wire),
        Err(Error::UnknownEventType(_))
    ));
}

#[test]
fn file_cited_with_lines_round_trips_through_json() {
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("percept-cli"),
        None,
        Timestamp::now(),
        Payload::FileCited {
            path: "src/mapstore/schema.rs".into(),
            lines: Some((40, 58)),
            excerpt: "fn parse() {}".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "file.cited");
    assert_eq!(wire.payload["lines"], "40-58");
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::FileCited {
            path,
            lines,
            excerpt,
        } => {
            assert_eq!(path.to_str().unwrap(), "src/mapstore/schema.rs");
            assert_eq!(*lines, Some((40, 58)));
            assert_eq!(excerpt, "fn parse() {}");
        }
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn file_cited_without_lines_round_trips_with_none() {
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("percept-cli"),
        None,
        Timestamp::now(),
        Payload::FileCited {
            path: "README.md".into(),
            lines: None,
            excerpt: "the whole file".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert!(wire.payload.get("lines").is_none());
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::FileCited { lines, .. } => assert_eq!(*lines, None),
        _ => panic!("expected FileCited"),
    }
}

#[test]
fn a_short_excerpt_summary_shows_it_whole() {
    let event = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("percept-cli"),
        None,
        Timestamp::now(),
        Payload::FileCited {
            path: "src/mapstore/schema.rs".into(),
            lines: Some((40, 58)),
            excerpt: "fn parse() {\n    todo!()\n}".to_string(),
        },
    );

    let line: Value = serde_json::from_str(&summarize(&event, None, PREVIEW_CHARS)).unwrap();
    assert_eq!(line["payload"]["path"], "src/mapstore/schema.rs");
    assert_eq!(line["payload"]["lines"], "40-58");
    assert_eq!(line["payload"]["excerpt"], "fn parse() {\n    todo!()\n}");
    assert!(line.get("preview").is_none(), "{line}");
}

#[test]
fn a_search_hit_inside_a_long_excerpt_carries_a_match_range() {
    let excerpt = format!("{}needle{}", "a".repeat(80), "b".repeat(80));
    let event = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("percept-cli"),
        None,
        Timestamp::now(),
        Payload::FileCited {
            path: "src/mapstore/schema.rs".into(),
            lines: None,
            excerpt: excerpt.clone(),
        },
    );

    let hit = 80..86;
    let line: Value =
        serde_json::from_str(&summarize(&event, Some(hit.clone()), PREVIEW_CHARS)).unwrap();

    assert_eq!(line["preview"]["len"], excerpt.chars().count());
    assert_eq!(line["preview"]["match"], hit.start);
    assert!(line["payload"]["excerpt"].as_str().unwrap().contains("needle"));
}

#[test]
fn a_ranged_read_on_a_citation_returns_those_lines() {
    let event = crate::core::Event::restore(
        EventId::new(),
        Actor::Agent,
        source("percept-cli"),
        None,
        Timestamp::now(),
        Payload::FileCited {
            path: "src/mapstore/schema.rs".into(),
            lines: None,
            excerpt: "one\ntwo\nthree".to_string(),
        },
    );

    let line: Value = serde_json::from_str(&excerpt(&event, Some(4), Some(7)).unwrap()).unwrap();

    assert_eq!(line["payload"]["excerpt"], "two");
    assert_eq!(line["preview"]["len"], 13);
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

#[test]
fn parse_lines_accepts_a_valid_range() {
    assert_eq!(parse_lines("2-3").unwrap(), (2, 3));
}

#[test]
fn parse_lines_rejects_a_reversed_range() {
    assert!(matches!(parse_lines("3-1"), Err(Error::BadLines(_))));
}

#[test]
fn parse_lines_rejects_a_zero_start() {
    assert!(matches!(parse_lines("0-1"), Err(Error::BadLines(_))));
}

#[test]
fn claim_confirmed_round_trips_through_json() {
    let node = NodeId::new();
    let me = human();
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(me),
        source("cli"),
        None,
        Timestamp::now(),
        Payload::ClaimConfirmed {
            map: "decisions".to_string(),
            node,
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "claim.confirmed");
    let restored = crate::store::from_wire(wire).unwrap();

    assert!(restored.actor() == Actor::Human(me));
    match restored.payload() {
        Payload::ClaimConfirmed {
            map,
            node: restored_node,
        } => {
            assert_eq!(map, "decisions");
            assert!(*restored_node == node);
        }
        _ => panic!("expected ClaimConfirmed"),
    }
}

#[test]
fn claim_disputed_round_trips_through_json() {
    let node = NodeId::new();
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(human()),
        source("cli"),
        None,
        Timestamp::now(),
        Payload::ClaimDisputed {
            map: "decisions".to_string(),
            node,
            why: "never proposed".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "claim.disputed");
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::ClaimDisputed {
            map,
            node: restored_node,
            why,
        } => {
            assert_eq!(map, "decisions");
            assert!(*restored_node == node);
            assert_eq!(why, "never proposed");
        }
        _ => panic!("expected ClaimDisputed"),
    }
}

#[test]
fn review_finished_round_trips_through_json() {
    let (a, b) = (NodeId::new(), NodeId::new());
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(human()),
        source("cli"),
        None,
        Timestamp::now(),
        Payload::ReviewFinished {
            map: "decisions".to_string(),
            nodes: vec![a, b],
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let wire: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(wire.kind, "review.finished");
    let restored = crate::store::from_wire(wire).unwrap();

    match restored.payload() {
        Payload::ReviewFinished { map, nodes } => {
            assert_eq!(map, "decisions");
            assert_eq!(nodes, &vec![a, b]);
        }
        _ => panic!("expected ReviewFinished"),
    }
}

#[test]
fn a_malformed_node_in_a_review_finished_payload_is_an_error() {
    let payload = serde_json::json!({
        "map": "decisions",
        "nodes": ["not-a-uuid"],
    });

    let err = match decode("user", source("cli"), "review.finished", None, payload, human()) {
        Err(e) => e,
        Ok(_) => panic!("expected a malformed node id to be rejected"),
    };
    assert!(matches!(err, Error::BadUuid(s) if s == "not-a-uuid"));
}

#[test]
fn a_file_cited_payload_with_a_reversed_range_fails_to_decode() {
    let source = source("percept-cli");
    let payload = serde_json::json!({
        "path": "src/lib.rs",
        "lines": "3-1",
        "excerpt": "text",
    });
    assert!(decode("model", source, "file.cited", None, payload, human()).is_err());
}

#[test]
fn a_human_without_an_id_round_trips_as_a_kind_alone() {
    let original = crate::core::Event::restore(
        EventId::new(),
        Actor::Human(None),
        source("cli"),
        None,
        Timestamp::now(),
        Payload::MessageReceived {
            content: "hi".to_string(),
        },
    );

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let line: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(line["actor"], serde_json::json!({"kind": "human"}));

    let restored = crate::store::from_wire(serde_json::from_str::<Event>(&json).unwrap()).unwrap();
    assert_eq!(restored.actor(), Actor::Human(None));
}

#[test]
fn a_legacy_user_string_reads_as_a_human_without_an_id() {
    let json = r#"{
        "id": "0192d1f0-1111-7000-8000-000000000000",
        "actor": "user",
        "source": {"name": "tui", "path": "/test"},
        "type": "message.received",
        "causation_id": null,
        "created_at": "2026-08-30T00:00:00Z",
        "payload": { "content": "hi" }
    }"#;

    let restored = crate::store::from_wire(serde_json::from_str::<Event>(json).unwrap()).unwrap();
    assert_eq!(restored.actor(), Actor::Human(None));
}

#[test]
fn a_line_with_a_log_cursor_round_trips_through_store_event() {
    let original = message(Actor::Agent, "hi".to_string());
    let cursor = crate::core::LogCursor {
        log: crate::core::LogId::new(),
        seq: 7,
    };

    let json = serde_json::to_string(&Event::from(&original)).unwrap();
    let mut wire: Event = serde_json::from_str(&json).unwrap();
    wire.log = Some(Cursor {
        id: cursor.log.as_uuid().to_string(),
        seq: cursor.seq,
    });

    let json = serde_json::to_string(&wire).unwrap();
    let decoded: Event = serde_json::from_str(&json).unwrap();
    let on_wire = decoded.log.unwrap();
    assert_eq!(on_wire.id, cursor.log.as_uuid().to_string());
    assert_eq!(on_wire.seq, cursor.seq);
}

#[test]
fn a_line_with_no_log_cursor_decodes_with_log_none() {
    let original = message(Actor::Agent, "hi".to_string());
    let json = serde_json::to_string(&Event::from(&original)).unwrap();

    let decoded: Event = serde_json::from_str(&json).unwrap();
    assert!(decoded.log.is_none());
}

#[test]
fn every_kind_names_round_trip_through_the_store_parser() {
    let kinds = [
        EventKind::MessageReceived,
        EventKind::ThoughtRecorded,
        EventKind::ToolCalled,
        EventKind::ToolResulted,
        EventKind::NodeAdded,
        EventKind::EdgeAdded,
        EventKind::ModelCalled,
        EventKind::SessionStarted,
        EventKind::FileCited,
        EventKind::ClaimConfirmed,
        EventKind::ClaimDisputed,
        EventKind::ReviewFinished,
    ];

    for kind in kinds {
        assert!(parse_kind(kind.name()).unwrap() == kind);
    }
}
