use serde_json::json;

use super::*;
use crate::percept::Actor;

#[test]
fn a_text_delta_parses_as_a_reply_chunk() {
    let line = r#"data: {"type":"response.output_text.delta","content_index":0,"delta":"Hi","item_id":"msg_1","output_index":0,"sequence_number":4}"#;
    match parse_line(line, "gpt-5").unwrap() {
        Line::Chunk(Chunk::Reply(content)) => assert_eq!(content, "Hi"),
        _ => panic!("expected a reply chunk"),
    }
}

#[test]
fn a_reasoning_summary_delta_parses_as_a_thought_chunk() {
    let line = r#"data: {"type":"response.reasoning_summary_text.delta","delta":"Weighing","item_id":"rs_1","output_index":0,"summary_index":0,"sequence_number":2}"#;
    match parse_line(line, "gpt-5").unwrap() {
        Line::Chunk(Chunk::Thought(thought)) => assert_eq!(thought, "Weighing"),
        _ => panic!("expected a thought chunk"),
    }
}

#[test]
fn a_finished_function_call_item_parses_as_a_tool_call_chunk() {
    let line = r#"data: {"type":"response.output_item.done","item":{"id":"fc_1","type":"function_call","status":"completed","arguments":"{\"size\":5}","call_id":"call_x","name":"search_events"},"output_index":0,"sequence_number":17}"#;
    match parse_line(line, "gpt-5").unwrap() {
        Line::Chunk(Chunk::ToolCall { tool, arguments }) => {
            assert_eq!(tool, "search_events");
            assert_eq!(
                serde_json::from_str::<Value>(&arguments).unwrap()["size"],
                5
            );
        }
        _ => panic!("expected a tool call chunk"),
    }
}

#[test]
fn a_finished_message_item_and_argument_fragments_yield_nothing() {
    let lines = [
        r#"data: {"type":"response.output_item.done","item":{"id":"msg_1","type":"message","status":"completed","content":[{"type":"output_text","text":"Hi"}],"role":"assistant"},"output_index":0,"sequence_number":9}"#,
        r#"data: {"type":"response.function_call_arguments.delta","delta":"{\"si","item_id":"fc_1","output_index":0,"sequence_number":3}"#,
        r#"data: {"type":"response.created","response":{"id":"resp_1","status":"in_progress"},"sequence_number":0}"#,
    ];
    for line in lines {
        assert!(
            matches!(parse_line(line, "gpt-5").unwrap(), Line::Empty),
            "{line}"
        );
    }
}

#[test]
fn a_completed_event_ends_the_stream_carrying_its_token_counts() {
    let line = r#"data: {"type":"response.completed","response":{"id":"resp_1","status":"completed","usage":{"input_tokens":12,"output_tokens":34,"input_tokens_details":{"cached_tokens":5}}},"sequence_number":20}"#;
    match parse_line(line, "gpt-5").unwrap() {
        Line::Done(usage) => {
            assert_eq!(usage.model, "gpt-5");
            assert_eq!(usage.input_tokens, 12);
            assert_eq!(usage.output_tokens, 34);
            assert_eq!(usage.cached_tokens, Some(5));
        }
        _ => panic!("expected done with counts"),
    }
}

#[test]
fn event_names_comments_and_blanks_are_skipped() {
    for line in [
        "event: response.output_text.delta",
        ": keep-alive",
        "",
        "\r",
    ] {
        assert!(
            matches!(parse_line(line, "gpt-5").unwrap(), Line::Empty),
            "{line:?}"
        );
    }
}

#[test]
fn an_incomplete_reply_is_an_error_naming_the_reason() {
    let line = r#"data: {"type":"response.incomplete","response":{"id":"resp_1","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}},"sequence_number":8}"#;
    let Err(err) = parse_line(line, "gpt-5") else {
        panic!("expected an error")
    };
    assert!(err.to_string().contains("max_output_tokens"));
}

#[test]
fn a_failed_reply_and_an_error_event_surface_what_the_server_said() {
    let failed = r#"data: {"type":"response.failed","response":{"id":"resp_1","status":"failed","error":{"code":"server_error","message":"upstream broke"}},"sequence_number":8}"#;
    let Err(err) = parse_line(failed, "gpt-5") else {
        panic!("expected an error")
    };
    assert!(err.to_string().contains("upstream broke"));
    let error = r#"data: {"type":"error","code":"rate_limit","message":"slow down","param":null,"sequence_number":1}"#;
    let Err(err) = parse_line(error, "gpt-5") else {
        panic!("expected an error")
    };
    assert!(err.to_string().contains("slow down"));
}

#[test]
fn a_malformed_event_is_an_error() {
    assert!(parse_line("data: not json", "gpt-5").is_err());
}

#[test]
fn a_tool_result_cites_the_call_before_it() {
    let messages = vec![
        Message::Text {
            role: Actor::User,
            content: "search".to_string(),
        },
        Message::ToolCall {
            tool: "search_events".to_string(),
            arguments: r#"{"size":5}"#.to_string(),
        },
        Message::ToolResult {
            content: "3 events".to_string(),
        },
        Message::ToolCall {
            tool: "read_event".to_string(),
            arguments: r#"{"id":"x"}"#.to_string(),
        },
        Message::ToolResult {
            content: "an event".to_string(),
        },
    ];

    let wire = serde_json::to_value(items(&messages)).unwrap();

    assert_eq!(
        wire[0],
        json!({"type": "message", "role": "user", "content": "search"})
    );
    assert_eq!(
        wire[1],
        json!({
            "type": "function_call",
            "call_id": "call_1",
            "name": "search_events",
            "arguments": "{\"size\":5}"
        })
    );
    assert_eq!(
        wire[2],
        json!({"type": "function_call_output", "call_id": "call_1", "output": "3 events"})
    );
    assert_eq!(wire[3]["call_id"], "call_2");
    assert_eq!(wire[4]["call_id"], "call_2");
}

#[test]
fn a_call_another_writer_left_unanswered_replays_as_text() {
    let messages = vec![
        Message::ToolCall {
            tool: "search_events".to_string(),
            arguments: r#"{"size":5}"#.to_string(),
        },
        Message::Text {
            role: Actor::User,
            content: "and now?".to_string(),
        },
    ];

    let wire = serde_json::to_value(items(&messages)).unwrap();

    assert_eq!(
        wire[0],
        json!({"type": "message", "role": "assistant", "content": "search_events({\"size\":5})"})
    );
}

#[test]
fn a_known_thinking_model_reports_thought_output() {
    let openai = openai("gpt-5.6-luna");
    assert!(openai.capabilities().output.contains(&Modality::Thought));
}

#[test]
fn an_unrecognized_model_is_text_only() {
    let openai = openai("gpt-3");
    assert_eq!(openai.capabilities().output, &[Modality::Text]);
    assert_eq!(openai.capabilities().context_window, None);
}

#[test]
fn a_request_carries_tools_flat_and_never_stores() {
    let tool = ToolSpec {
        name: "search_events",
        description: "search",
        parameters: r#"{"type":"object"}"#,
    };
    let request = ModelRequest {
        messages: Vec::new(),
        tools: vec![tool],
    };

    let wire =
        serde_json::to_value(Request::new("m".to_string(), "low".to_string(), &request)).unwrap();

    assert_eq!(
        wire["tools"][0],
        json!({"type": "function", "name": "search_events", "description": "search", "parameters": {"type": "object"}})
    );
    assert_eq!(
        wire["reasoning"],
        json!({"effort": "low", "summary": "auto"})
    );
    assert_eq!(wire["store"], false);
    assert_eq!(wire["parallel_tool_calls"], false);
    assert_eq!(wire["stream"], true);
}

fn openai(model: &str) -> OpenAi {
    OpenAi::new(
        "https://api.openai.com/v1".to_string(),
        model.to_string(),
        "low".to_string(),
        "key".to_string(),
    )
}
