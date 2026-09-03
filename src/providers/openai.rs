use std::error::Error;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{client, forward, role, stream_lines, Line};
use crate::percept::{
    Chunk, Message, Modality, Model, ModelCapabilities, ModelRequest, ReplyStream, ToolSpec, Usage,
};

/// Sends and receives with OpenAI's `/responses`. A streamed reply is
/// server-sent events, one `data:` line per JSON object, each named
/// by its `type`. Nothing is stored server-side: the log is the
/// conversation, and every turn replays it.
pub struct OpenAi {
    url: String,
    model: String,
    reasoning_effort: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAi {
    /// `reasoning_effort` is the API's word - `none`, `low`, `medium`,
    /// `high` - for how long the model thinks before it answers.
    pub fn new(base_url: String, model: String, reasoning_effort: String, api_key: String) -> Self {
        Self {
            url: format!("{base_url}/responses"),
            model,
            reasoning_effort,
            api_key,
            client: client(),
        }
    }
}

#[derive(Serialize)]
struct Request {
    model: String,
    input: Vec<Item>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDef>,
    /// Off because the loop runs tools one at a time, so a second call
    /// in one reply would never run.
    parallel_tool_calls: bool,
    reasoning: Reasoning,
    stream: bool,
    store: bool,
}

impl Request {
    fn new(model: String, reasoning_effort: String, request: &ModelRequest) -> Self {
        Self {
            model,
            input: items(&request.messages),
            tools: request.tools.iter().map(tool_def).collect(),
            parallel_tool_calls: false,
            reasoning: Reasoning {
                effort: reasoning_effort,
                summary: "auto",
            },
            stream: true,
            store: false,
        }
    }
}

#[derive(Serialize)]
struct Reasoning {
    effort: String,
    /// Asks for a summary of the reasoning, which streams as thoughts.
    /// Absent from what a model that writes none sends.
    summary: &'static str,
}

#[derive(Serialize)]
struct ToolDef {
    #[serde(rename = "type")]
    kind: &'static str,
    name: &'static str,
    description: &'static str,
    /// `ToolSpec` carries the schema as text; the wire wants an object.
    parameters: Value,
}

fn tool_def(spec: &ToolSpec) -> ToolDef {
    ToolDef {
        kind: "function",
        name: spec.name,
        description: spec.description,
        parameters: serde_json::from_str(spec.parameters)
            .expect("ToolSpec parameters is a JSON Schema literal"),
    }
}

/// One item of the conversation as `/responses` takes it.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Item {
    Message {
        role: &'static str,
        content: String,
    },
    FunctionCall {
        call_id: String,
        name: String,
        /// JSON text, the same shape the domain carries.
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        output: String,
    },
}

/// The domain keeps no id for a tool call, but the API ties a result
/// to its call by one. Calls are numbered in transcript order and a
/// result cites the call before it - `to_messages` never yields a
/// result with no call ahead of it. The reverse happens: a call
/// another writer logged has no result here, and the API refuses a
/// call left unanswered, so it replays as the model's text instead.
fn items(messages: &[Message]) -> Vec<Item> {
    let mut calls = 0;
    let mut out = Vec::with_capacity(messages.len());
    let mut messages = messages.iter().peekable();
    while let Some(message) = messages.next() {
        out.push(match message {
            Message::Text {
                role: actor,
                content,
            } => Item::Message {
                role: role(*actor),
                content: content.clone(),
            },
            Message::ToolCall { tool, arguments }
                if !matches!(messages.peek(), Some(Message::ToolResult { .. })) =>
            {
                Item::Message {
                    role: "assistant",
                    content: format!("{tool}({arguments})"),
                }
            }
            Message::ToolCall { tool, arguments } => {
                calls += 1;
                Item::FunctionCall {
                    call_id: format!("call_{calls}"),
                    name: tool.clone(),
                    arguments: arguments.clone(),
                }
            }
            Message::ToolResult { content } => Item::FunctionCallOutput {
                call_id: format!("call_{calls}"),
                output: content.clone(),
            },
        });
    }
    out
}

/// The streamed events OpenAi acts on, by their `type`. Everything
/// else the server sends - lifecycle, content parts, argument
/// fragments of a call that arrives whole in its item - is `Other`.
#[derive(Deserialize)]
#[serde(tag = "type")]
enum StreamEvent {
    #[serde(rename = "response.output_text.delta")]
    Text { delta: String },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ReasoningSummary { delta: String },
    #[serde(rename = "response.reasoning_text.delta")]
    Reasoning { delta: String },
    #[serde(rename = "response.output_item.done")]
    ItemDone { item: OutputItem },
    #[serde(rename = "response.completed")]
    Completed { response: CompletedResponse },
    #[serde(rename = "response.incomplete")]
    Incomplete { response: IncompleteResponse },
    #[serde(rename = "response.failed")]
    Failed { response: FailedResponse },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum OutputItem {
    #[serde(rename = "function_call")]
    FunctionCall { name: String, arguments: String },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct CompletedResponse {
    usage: ResponseUsage,
}

#[derive(Deserialize)]
struct ResponseUsage {
    input_tokens: u64,
    output_tokens: u64,
    input_tokens_details: InputTokensDetails,
}

#[derive(Deserialize)]
struct InputTokensDetails {
    cached_tokens: u64,
}

#[derive(Deserialize)]
struct IncompleteResponse {
    incomplete_details: IncompleteDetails,
}

#[derive(Deserialize)]
struct IncompleteDetails {
    reason: String,
}

#[derive(Deserialize)]
struct FailedResponse {
    error: ApiError,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

fn parse_line(line: &str, model: &str) -> Result<Line, Box<dyn Error + Send + Sync>> {
    let Some(data) = line.trim_end_matches('\r').strip_prefix("data:") else {
        // The `event:` line naming what the next `data:` carries, a
        // comment, or the blank between events.
        return Ok(Line::Empty);
    };
    let event: StreamEvent = serde_json::from_str(data.trim())
        .map_err(|err| format!("malformed event from openai: {err}"))?;
    Ok(match event {
        StreamEvent::Text { delta } => Line::Chunk(Chunk::Reply(delta)),
        StreamEvent::ReasoningSummary { delta } | StreamEvent::Reasoning { delta } => {
            Line::Chunk(Chunk::Thought(delta))
        }
        StreamEvent::ItemDone {
            item: OutputItem::FunctionCall { name, arguments },
        } => Line::Chunk(Chunk::ToolCall {
            tool: name,
            arguments,
        }),
        StreamEvent::ItemDone { .. } | StreamEvent::Other => Line::Empty,
        StreamEvent::Completed { response } => Line::Done(Usage {
            model: model.to_string(),
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            cached_tokens: Some(response.usage.input_tokens_details.cached_tokens),
        }),
        StreamEvent::Incomplete { response } => {
            return Err(format!(
                "openai cut the reply off: {}",
                response.incomplete_details.reason
            )
            .into())
        }
        StreamEvent::Failed { response } => {
            return Err(format!("openai failed the reply: {}", response.error.message).into())
        }
        StreamEvent::Error { message } => return Err(format!("openai reported: {message}").into()),
    })
}

impl Model for OpenAi {
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            input: &[Modality::Text],
            output: &[Modality::Text],
            tool_use: true,
        }
    }

    fn reply(&self, request: &ModelRequest) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let client = self.client.clone();
        let url = self.url.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let request = Request::new(self.model.clone(), self.reasoning_effort.clone(), request);

        tokio::spawn(async move {
            let request = client.post(&url).bearer_auth(api_key).json(&request);
            let mut pending = None;
            stream_lines(request, "openai", &tx, |line| {
                forward(parse_line(line, &model), &tx, &mut pending)
            })
            .await;
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
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

        let wire = serde_json::to_value(Request::new("m".to_string(), "low".to_string(), &request))
            .unwrap();

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
}
