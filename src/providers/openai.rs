use std::error::Error;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use super::{client, take_lines, tool_def, ToolDef};
use crate::percept::{
    Actor, Chunk, Message, Modality, Model, ModelCapabilities, ModelRequest, ReplyStream,
};

/// Sends and receives with OpenAI's `/chat/completions`, or any server
/// that speaks it. A streamed reply is server-sent events: one `data:`
/// line per JSON object, ending on `data: [DONE]`.
pub struct OpenAi {
    url: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAi {
    pub fn new(base_url: String, model: String, api_key: String) -> Self {
        Self {
            url: format!("{base_url}/chat/completions"),
            model,
            api_key,
            client: client(),
        }
    }
}

/// OpenAI's role vocabulary - a string, not an enum, because it's the
/// wire's word, not the domain's.
fn role(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
        Actor::Model => "assistant",
        Actor::System => "system",
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDef>,
    /// Only sent with tools: the API refuses it on a plain chat
    /// request. Off because the loop runs tools one at a time, so a
    /// second call in one message would never run.
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    /// `None` on the model's tool call turn, where the API wants null
    /// rather than an empty string.
    content: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
    /// A tool's output names the call it answers.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: ToolCallFunction,
}

#[derive(Serialize)]
struct ToolCallFunction {
    name: String,
    /// JSON text, the same shape the domain carries.
    arguments: String,
}

/// The domain keeps no id for a tool call, but the API ties a result
/// to its call by one. Calls are numbered in transcript order and a
/// result cites the call before it - `to_messages` never yields a
/// result with no call ahead of it.
fn chat_messages(messages: &[Message]) -> Vec<ChatMessage> {
    let mut calls = 0;
    messages
        .iter()
        .map(|message| match message {
            Message::Text {
                role: actor,
                content,
            } => ChatMessage {
                role: role(*actor),
                content: Some(content.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
            Message::ToolCall { tool, arguments } => {
                calls += 1;
                ChatMessage {
                    role: "assistant",
                    content: None,
                    tool_calls: vec![ToolCall {
                        id: format!("call_{calls}"),
                        kind: "function",
                        function: ToolCallFunction {
                            name: tool.clone(),
                            arguments: arguments.clone(),
                        },
                    }],
                    tool_call_id: None,
                }
            }
            Message::ToolResult { content } => ChatMessage {
                role: "tool",
                content: Some(content.clone()),
                tool_calls: Vec::new(),
                tool_call_id: Some(format!("call_{calls}")),
            },
        })
        .collect()
}

/// One streamed event's JSON. Only what OpenAi reads is deserialized.
#[derive(Deserialize)]
struct StreamChunk {
    #[serde(default)]
    choices: Vec<Choice>,
    /// A failure after the headers went out arrives as an event, not a
    /// status.
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct Choice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    /// A tool call arrives in pieces: the first delta names it, the
    /// rest carry fragments of its arguments.
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

#[derive(Deserialize)]
struct ToolCallDelta {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

/// What one line means for the stream: a chunk to forward, the
/// sentinel that ends it, or nothing - a comment, a blank, a delta with
/// no text, a fragment of a tool call still being assembled.
enum Line {
    Chunk(Chunk),
    Empty,
    Done,
}

/// Reads the event stream line by line, assembling the one tool call
/// the loop will run out of its deltas.
#[derive(Default)]
struct Decoder {
    tool: String,
    arguments: String,
    /// Whether any tool call delta has arrived this reply.
    calling: bool,
}

impl Decoder {
    fn decode(&mut self, line: &str) -> Result<Line, Box<dyn Error + Send + Sync>> {
        let Some(data) = line.trim_end_matches('\r').strip_prefix("data:") else {
            // A comment, a blank between events, or a field OpenAi
            // never reads, such as `event:`.
            return Ok(Line::Empty);
        };
        let data = data.trim();
        if data == "[DONE]" {
            return Ok(Line::Done);
        }
        let raw: StreamChunk = serde_json::from_str(data)
            .map_err(|err| format!("malformed event from openai: {err}"))?;
        if let Some(error) = raw.error {
            return Err(format!("openai reported: {}", error.message).into());
        }
        let Some(choice) = raw.choices.into_iter().next() else {
            return Ok(Line::Empty);
        };

        // Only the first call is assembled - the loop runs tools one at
        // a time. `parallel_tool_calls` is off, so a second one is a
        // server ignoring the request; the log records just the first,
        // and its replayed history stays consistent.
        for delta in choice.delta.tool_calls {
            if delta.index != 0 {
                continue;
            }
            self.calling = true;
            if let Some(function) = delta.function {
                if let Some(name) = function.name {
                    self.tool.push_str(&name);
                }
                if let Some(arguments) = function.arguments {
                    self.arguments.push_str(&arguments);
                }
            }
        }

        match choice.finish_reason.as_deref() {
            Some("length") => Err("openai cut the reply off at its token limit".into()),
            Some(_) => Ok(self.take_call().map_or(Line::Empty, Line::Chunk)),
            None => match choice.delta.content {
                Some(content) if !content.is_empty() => Ok(Line::Chunk(Chunk::Reply(content))),
                _ => Ok(Line::Empty),
            },
        }
    }

    /// The assembled call, once, if any delta started one.
    fn take_call(&mut self) -> Option<Chunk> {
        if !self.calling {
            return None;
        }
        self.calling = false;
        Some(Chunk::ToolCall {
            tool: std::mem::take(&mut self.tool),
            arguments: std::mem::take(&mut self.arguments),
        })
    }
}

/// Decodes and forwards one line. Returns `true` once the stream is
/// over - the `[DONE]` sentinel, a failure, or the receiver having gone
/// away - so the caller knows to stop reading the body.
fn handle_line(
    decoder: &mut Decoder,
    line: &str,
    tx: &UnboundedSender<Result<Chunk, Box<dyn Error + Send + Sync>>>,
) -> bool {
    match decoder.decode(line) {
        Ok(Line::Chunk(chunk)) => tx.send(Ok(chunk)).is_err(),
        Ok(Line::Empty) => false,
        Ok(Line::Done) => {
            // A server that ends without a finish reason still owes the
            // call it started.
            if let Some(call) = decoder.take_call() {
                let _ = tx.send(Ok(call));
            }
            true
        }
        Err(err) => {
            let _ = tx.send(Err(err));
            true
        }
    }
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
        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages(&request.messages),
            parallel_tool_calls: (!request.tools.is_empty()).then_some(false),
            tools: request.tools.iter().map(tool_def).collect(),
            stream: true,
        };

        tokio::spawn(async move {
            let response = match client
                .post(&url)
                .bearer_auth(api_key)
                .json(&request)
                .send()
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    let _ = tx.send(Err(format!("request to openai failed: {err}").into()));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx.send(Err(format!("openai returned {status}: {body}").into()));
                return;
            }

            let mut decoder = Decoder::default();
            let mut body = response.bytes_stream();
            let mut buf = Vec::new();
            while let Some(chunk) = body.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        let _ = tx.send(Err(format!("openai stream failed: {err}").into()));
                        return;
                    }
                };
                for line in take_lines(&mut buf, &chunk) {
                    if handle_line(&mut decoder, &line, &tx) {
                        return;
                    }
                }
            }

            // Whatever the body ended on without a trailing newline is
            // still a line; an empty tail decodes as nothing.
            handle_line(&mut decoder, &String::from_utf8_lossy(&buf), &tx);
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn decode(decoder: &mut Decoder, line: &str) -> Line {
        decoder.decode(line).unwrap()
    }

    #[test]
    fn a_content_delta_decodes_as_a_reply_chunk() {
        let line =
            r#"data: {"choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#;
        match decode(&mut Decoder::default(), line) {
            Line::Chunk(Chunk::Reply(content)) => assert_eq!(content, "Hi"),
            _ => panic!("expected a reply chunk"),
        }
    }

    #[test]
    fn a_tool_call_is_assembled_from_its_deltas_and_emitted_on_finish() {
        let mut decoder = Decoder::default();
        let lines = [
            r#"data: {"choices":[{"delta":{"role":"assistant","content":null,"tool_calls":[{"index":0,"id":"call_x","type":"function","function":{"name":"search_events","arguments":""}}]},"finish_reason":null}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"si"}}]},"finish_reason":null}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ze\":5}"}}]},"finish_reason":null}]}"#,
        ];
        for line in lines {
            assert!(matches!(decode(&mut decoder, line), Line::Empty));
        }
        let finish = r#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        match decode(&mut decoder, finish) {
            Line::Chunk(Chunk::ToolCall { tool, arguments }) => {
                assert_eq!(tool, "search_events");
                let args: Value = serde_json::from_str(&arguments).unwrap();
                assert_eq!(args["size"], 5);
            }
            _ => panic!("expected a tool call chunk"),
        }
        assert!(decoder.take_call().is_none());
    }

    #[test]
    fn a_second_parallel_call_is_ignored() {
        let mut decoder = Decoder::default();
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"read_event","arguments":"{}"}},{"index":1,"function":{"name":"search_events","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#;
        match decode(&mut decoder, line) {
            Line::Chunk(Chunk::ToolCall { tool, .. }) => assert_eq!(tool, "read_event"),
            _ => panic!("expected a tool call chunk"),
        }
    }

    #[test]
    fn a_stop_with_no_call_pending_yields_nothing() {
        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        assert!(matches!(decode(&mut Decoder::default(), line), Line::Empty));
    }

    #[test]
    fn the_done_sentinel_ends_the_stream() {
        assert!(matches!(
            decode(&mut Decoder::default(), "data: [DONE]"),
            Line::Done
        ));
    }

    #[test]
    fn a_call_still_pending_at_done_is_flushed() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut decoder = Decoder::default();
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"read_event","arguments":"{}"}}]},"finish_reason":null}]}"#;
        assert!(!handle_line(&mut decoder, line, &tx));
        assert!(handle_line(&mut decoder, "data: [DONE]", &tx));
        match rx.try_recv().unwrap().unwrap() {
            Chunk::ToolCall { tool, .. } => assert_eq!(tool, "read_event"),
            _ => panic!("expected a tool call chunk"),
        }
    }

    #[test]
    fn comments_blanks_and_other_fields_are_skipped() {
        let mut decoder = Decoder::default();
        for line in [": keep-alive", "", "event: message", "\r"] {
            assert!(matches!(decode(&mut decoder, line), Line::Empty));
        }
    }

    #[test]
    fn a_reply_cut_at_the_token_limit_is_an_error() {
        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"length"}]}"#;
        assert!(Decoder::default().decode(line).is_err());
    }

    #[test]
    fn an_error_event_surfaces_what_the_server_said() {
        let line = r#"data: {"error":{"message":"The model `nope` does not exist","type":"invalid_request_error"}}"#;
        let Err(err) = Decoder::default().decode(line) else {
            panic!("expected an error")
        };
        assert!(err.to_string().contains("does not exist"));
    }

    #[test]
    fn a_malformed_event_is_an_error() {
        assert!(Decoder::default().decode("data: not json").is_err());
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

        let wire = serde_json::to_value(chat_messages(&messages)).unwrap();

        assert_eq!(wire[0], json!({"role": "user", "content": "search"}));
        assert_eq!(
            wire[1],
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "search_events", "arguments": "{\"size\":5}"}
                }]
            })
        );
        assert_eq!(
            wire[2],
            json!({"role": "tool", "content": "3 events", "tool_call_id": "call_1"})
        );
        assert_eq!(wire[3]["tool_calls"][0]["id"], "call_2");
        assert_eq!(wire[4]["tool_call_id"], "call_2");
    }

    #[test]
    fn parallel_tool_calls_is_only_sent_with_tools() {
        let plain = ChatRequest {
            model: "m".to_string(),
            messages: Vec::new(),
            tools: Vec::new(),
            parallel_tool_calls: None,
            stream: true,
        };
        let wire = serde_json::to_value(&plain).unwrap();
        assert!(wire.get("parallel_tool_calls").is_none());
        assert!(wire.get("tools").is_none());
    }

    #[test]
    fn roles_map_to_openais_vocabulary() {
        assert_eq!(role(Actor::User), "user");
        assert_eq!(role(Actor::Model), "assistant");
        assert_eq!(role(Actor::System), "system");
    }
}
