use std::error::Error;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{client, forward, role, stream_lines, Line};
use crate::percept::{
    Chunk, Message, Modality, Model, ModelCapabilities, ModelRequest, ReplyStream, ToolSpec, Usage,
};

/// Sends and receives with Fireworks' OpenAI-compatible `/chat/completions`.
/// A streamed reply is server-sent events, one `data:` line per JSON
/// object, ended by a literal `data: [DONE]`.
pub struct Fireworks {
    url: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl Fireworks {
    pub fn new(base_url: String, model: String, api_key: String) -> Self {
        Self {
            url: format!("{base_url}/chat/completions"),
            model,
            api_key,
            client: client(),
        }
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDef>,
    stream: bool,
    stream_options: StreamOptions,
}

#[derive(Serialize)]
struct StreamOptions {
    /// Without this, the stream never carries a `usage` field, and a
    /// reply's cost would have to be reported as zero.
    include_usage: bool,
}

#[derive(Serialize)]
struct ToolDef {
    #[serde(rename = "type")]
    kind: &'static str,
    function: ToolDefFunction,
}

#[derive(Serialize)]
struct ToolDefFunction {
    name: &'static str,
    description: &'static str,
    /// `ToolSpec` carries the schema as text; the wire wants an object.
    parameters: Value,
}

fn tool_def(spec: &ToolSpec) -> ToolDef {
    ToolDef {
        kind: "function",
        function: ToolDefFunction {
            name: spec.name,
            description: spec.description,
            parameters: serde_json::from_str(spec.parameters)
                .expect("ToolSpec parameters is a JSON Schema literal"),
        },
    }
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OutgoingToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// One tool call as replayed back to Fireworks: complete, unlike a
/// streamed delta, which arrives in fragments (see `ToolCallDelta`).
#[derive(Serialize)]
struct OutgoingToolCall {
    id: String,
    function: OutgoingToolCallFunction,
}

#[derive(Serialize)]
struct OutgoingToolCallFunction {
    name: String,
    arguments: String,
}

/// The domain keeps no id for a tool call, but the API ties a result to
/// its call by one. Calls are numbered in transcript order and a result
/// cites the call before it - the same scheme `openai.rs` uses. A call
/// with no result ahead of the next message (the log ends mid-turn,
/// e.g. after a crash) replays as the model's text instead: Fireworks
/// rejects a `tool_calls` message whose id nothing answers.
fn chat_messages(messages: &[Message]) -> Vec<ChatMessage> {
    let mut calls = 0;
    let mut out = Vec::with_capacity(messages.len());
    let mut messages = messages.iter().peekable();
    while let Some(message) = messages.next() {
        out.push(match message {
            Message::Text {
                role: actor,
                content,
            } => ChatMessage {
                role: role(*actor),
                content: content.clone(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
            Message::ToolCall { tool, arguments }
                if !matches!(messages.peek(), Some(Message::ToolResult { .. })) =>
            {
                ChatMessage {
                    role: "assistant",
                    content: format!("{tool}({arguments})"),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                }
            }
            Message::ToolCall { tool, arguments } => {
                calls += 1;
                ChatMessage {
                    role: "assistant",
                    content: String::new(),
                    tool_calls: vec![OutgoingToolCall {
                        id: format!("call_{calls}"),
                        function: OutgoingToolCallFunction {
                            name: tool.clone(),
                            arguments: arguments.clone(),
                        },
                    }],
                    tool_call_id: None,
                }
            }
            Message::ToolResult { content } => ChatMessage {
                role: "tool",
                content: content.clone(),
                tool_calls: Vec::new(),
                tool_call_id: Some(format!("call_{calls}")),
            },
        });
    }
    out
}

/// One streamed chunk of `/chat/completions`, SSE-wrapped. Only the
/// fields Fireworks reports on are deserialized.
#[derive(Deserialize)]
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
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
    content: String,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

/// One fragment of a streamed tool call. Fireworks, like OpenAI's chat
/// completions, sends the name once on the first fragment and the
/// arguments split across many - never a whole call in one delta, so a
/// call is assembled in `Accumulator` rather than read off one line.
/// `index` says which of the round's parallel calls a fragment belongs
/// to.
#[derive(Deserialize)]
struct ToolCallDelta {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    function: ToolCallDeltaFunction,
}

#[derive(Deserialize, Default)]
struct ToolCallDeltaFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: String,
}

#[derive(Deserialize)]
struct ChunkUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
}

/// Assembles a tool call across its streamed fragments: the name from
/// the first, arguments concatenated from every one, until a
/// `finish_reason` of `tool_calls` says the call is whole. Only the
/// round's first call, index 0, is assembled: percept runs one call
/// per round and asks again, so a parallel call is dropped here rather
/// than spliced into the first one's arguments - the model sees one
/// result and asks for the rest.
#[derive(Default)]
struct Accumulator {
    tool: String,
    arguments: String,
}

fn parse_line(
    line: &str,
    model: &str,
    partial: &mut Option<Accumulator>,
) -> Result<Line, Box<dyn Error + Send + Sync>> {
    let Some(data) = line.trim_end_matches('\r').strip_prefix("data:") else {
        return Ok(Line::Empty);
    };
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Line::Empty);
    }
    let chunk: ChatCompletionChunk = serde_json::from_str(data)
        .map_err(|err| format!("malformed chunk from fireworks: {err}"))?;
    if let Some(usage) = chunk.usage {
        return Ok(Line::Done(Usage {
            model: model.to_string(),
            input_tokens: usage.prompt_tokens,
            output_tokens: usage.completion_tokens,
            cached_tokens: None,
        }));
    }
    let Some(choice) = chunk.choices.into_iter().next() else {
        return Ok(Line::Empty);
    };
    for delta in choice
        .delta
        .tool_calls
        .into_iter()
        .filter(|delta| delta.index == 0)
    {
        let accumulator = partial.get_or_insert_with(Accumulator::default);
        if let Some(name) = delta.function.name {
            accumulator.tool = name;
        }
        accumulator.arguments.push_str(&delta.function.arguments);
    }
    if choice.finish_reason.as_deref() == Some("tool_calls") {
        let accumulator = partial
            .take()
            .ok_or("fireworks finished a tool call it never started")?;
        return Ok(Line::Chunk(Chunk::ToolCall {
            tool: accumulator.tool,
            arguments: accumulator.arguments,
        }));
    }
    if !choice.delta.content.is_empty() {
        Ok(Line::Chunk(Chunk::Reply(choice.delta.content)))
    } else {
        Ok(Line::Empty)
    }
}

impl Model for Fireworks {
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            input: &[Modality::Text],
            output: &[Modality::Text],
            tool_use: true,
            reasoning_efforts: &[],
            default_reasoning_effort: None,
            context_window: None,
        }
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn reply(&self, request: &ModelRequest) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let client = self.client.clone();
        let url = self.url.clone();
        let api_key = self.api_key.clone();
        let model = self.model.clone();
        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages(&request.messages),
            tools: request.tools.iter().map(tool_def).collect(),
            stream: true,
            stream_options: StreamOptions {
                include_usage: true,
            },
        };

        tokio::spawn(async move {
            let request = client.post(&url).bearer_auth(api_key).json(&request);
            let mut pending = None;
            let mut partial_call = None;
            stream_lines(request, "fireworks", &tx, |line| {
                forward(
                    parse_line(line, &model, &mut partial_call),
                    &tx,
                    &mut pending,
                )
            })
            .await;
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests;
