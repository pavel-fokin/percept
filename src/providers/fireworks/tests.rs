use serde_json::json;

use super::*;
use crate::core::testing::human;
use crate::core::Actor;

#[test]
fn a_content_delta_parses_as_a_reply_chunk() {
    let line = r#"data: {"choices":[{"delta":{"content":"Hi"}}]}"#;
    let mut partial = None;
    match parse_line(line, "glm-5p3", &mut partial).unwrap() {
        Line::Chunk(Chunk::Reply(content)) => assert_eq!(content, "Hi"),
        _ => panic!("expected a reply chunk"),
    }
}

#[test]
fn a_tool_call_split_across_deltas_assembles_into_one_call() {
    let mut partial = None;

    let first = r#"data: {"choices":[{"delta":{"tool_calls":[{"function":{"name":"search_events","arguments":""}}]}}]}"#;
    assert!(matches!(
        parse_line(first, "glm-5p3", &mut partial).unwrap(),
        Line::Empty
    ));

    let second =
        r#"data: {"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"{\"si"}}]}}]}"#;
    assert!(matches!(
        parse_line(second, "glm-5p3", &mut partial).unwrap(),
        Line::Empty
    ));

    let third = r#"data: {"choices":[{"delta":{"tool_calls":[{"function":{"arguments":"ze\":5}"}}]},"finish_reason":"tool_calls"}]}"#;
    match parse_line(third, "glm-5p3", &mut partial).unwrap() {
        Line::Chunk(Chunk::ToolCall { tool, arguments }) => {
            assert_eq!(tool, "search_events");
            assert_eq!(
                serde_json::from_str::<Value>(&arguments).unwrap()["size"],
                5
            );
        }
        _ => panic!("expected a tool call chunk"),
    }
    assert!(
        partial.is_none(),
        "the accumulator is cleared after the call"
    );
}

#[test]
fn a_second_parallel_call_is_dropped_rather_than_spliced_into_the_first() {
    let mut partial = None;

    let first = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"read_file","arguments":"{\"path\":\"a\"}"}}]}}]}"#;
    let _ = parse_line(first, "glm-5p3", &mut partial).unwrap();
    let second = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":1,"function":{"name":"read_file","arguments":"{\"path\":\"b\"}"}}]},"finish_reason":"tool_calls"}]}"#;

    match parse_line(second, "glm-5p3", &mut partial).unwrap() {
        Line::Chunk(Chunk::ToolCall { tool, arguments }) => {
            assert_eq!(tool, "read_file");
            assert_eq!(
                serde_json::from_str::<Value>(&arguments).unwrap()["path"],
                "a"
            );
        }
        _ => panic!("expected a tool call chunk"),
    }
}

#[test]
fn a_chunk_with_usage_ends_the_stream_carrying_its_token_counts() {
    let line = r#"data: {"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":34}}"#;
    let mut partial = None;
    match parse_line(line, "glm-5p3", &mut partial).unwrap() {
        Line::Done(usage) => {
            assert_eq!(usage.model, "glm-5p3");
            assert_eq!(usage.input_tokens, 12);
            assert_eq!(usage.output_tokens, 34);
            assert_eq!(usage.cached_tokens, None);
        }
        _ => panic!("expected done with counts"),
    }
}

#[test]
fn the_done_sentinel_with_no_usage_yields_nothing() {
    let mut partial = None;
    assert!(matches!(
        parse_line("data: [DONE]", "glm-5p3", &mut partial).unwrap(),
        Line::Empty
    ));
}

#[test]
fn a_chunk_with_no_choices_and_no_usage_yields_nothing() {
    let line = r#"data: {"choices":[]}"#;
    let mut partial = None;
    assert!(matches!(
        parse_line(line, "glm-5p3", &mut partial).unwrap(),
        Line::Empty
    ));
}

#[test]
fn a_blank_line_yields_nothing() {
    let mut partial = None;
    assert!(matches!(
        parse_line("", "glm-5p3", &mut partial).unwrap(),
        Line::Empty
    ));
}

#[test]
fn a_malformed_line_is_an_error() {
    let mut partial = None;
    assert!(parse_line("data: not json", "glm-5p3", &mut partial).is_err());
}

#[test]
fn a_tool_result_cites_the_call_before_it() {
    let messages = vec![
        Message::Text {
            role: Actor::Human(human()),
            content: "search".to_string(),
        },
        Message::ToolCall {
            tool: "search_events".to_string(),
            arguments: r#"{"size":5}"#.to_string(),
        },
        Message::ToolResult {
            content: "3 events".to_string(),
        },
    ];

    let wire = serde_json::to_value(chat_messages(&messages)).unwrap();

    assert_eq!(wire[0], json!({"role": "user", "content": "search"}));
    assert_eq!(
        wire[1]["tool_calls"][0]["function"],
        json!({"name": "search_events", "arguments": "{\"size\":5}"})
    );
    assert_eq!(wire[2]["role"], "tool");
    assert_eq!(wire[2]["tool_call_id"], "call_1");
}

#[test]
fn a_call_left_unanswered_replays_as_text() {
    let messages = vec![
        Message::ToolCall {
            tool: "search_events".to_string(),
            arguments: r#"{"size":5}"#.to_string(),
        },
        Message::Text {
            role: Actor::Human(human()),
            content: "and now?".to_string(),
        },
    ];

    let wire = serde_json::to_value(chat_messages(&messages)).unwrap();

    assert_eq!(
        wire[0],
        json!({"role": "assistant", "content": "search_events({\"size\":5})"})
    );
}

#[test]
fn a_request_carries_tools_and_asks_for_usage() {
    let tool = ToolSpec {
        name: "search_events",
        description: "search",
        parameters: r#"{"type":"object"}"#,
    };
    let request = ModelRequest {
        messages: Vec::new(),
        tools: vec![tool],
        reasoning_effort: None,
    };

    let wire = serde_json::to_value(ChatRequest {
        model: "glm-5p3".to_string(),
        messages: chat_messages(&request.messages),
        tools: request.tools.iter().map(tool_def).collect(),
        stream: true,
        stream_options: StreamOptions {
            include_usage: true,
        },
    })
    .unwrap();

    assert_eq!(
        wire["tools"][0],
        json!({"type": "function", "function": {"name": "search_events", "description": "search", "parameters": {"type": "object"}}})
    );
    assert_eq!(wire["stream"], true);
    assert_eq!(wire["stream_options"], json!({"include_usage": true}));
}

#[test]
fn capabilities_report_text_only_with_tool_use() {
    let fireworks = Fireworks::new(
        "https://api.fireworks.ai/inference/v1".to_string(),
        "accounts/fireworks/models/glm-5p3".to_string(),
        "key".to_string(),
    );
    let capabilities = fireworks.capabilities();
    assert_eq!(capabilities.output, &[Modality::Text]);
    assert!(capabilities.tool_use);
}
