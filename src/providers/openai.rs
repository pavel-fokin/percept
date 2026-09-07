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

impl Model for OpenAi {
    fn capabilities(&self) -> ModelCapabilities {
        let ModelProfile {
            context_window,
            output,
        } = model_profile(&self.model).unwrap_or(ModelProfile {
            context_window: None,
            output: &[Modality::Text],
        });
        ModelCapabilities {
            input: &[Modality::Text],
            output,
            tool_use: true,
            context_window,
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
        let request = Request::new(model.clone(), self.reasoning_effort.clone(), request);

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

/// OpenAI models this app knows the shape of - Sol, Terra, and Luna
/// all share one context window and all think, so `reply` sends
/// `reasoning.effort` and parses reasoning deltas for each.
const GPT_5_6_FAMILY: &[&str] = &["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];

/// What this app knows about one model name.
struct ModelProfile {
    context_window: Option<u32>,
    output: &'static [Modality],
}

/// `None` for a name the table doesn't recognize - not a guess.
fn model_profile(model: &str) -> Option<ModelProfile> {
    GPT_5_6_FAMILY.contains(&model).then_some(ModelProfile {
        context_window: Some(1_050_000),
        output: &[Modality::Text, Modality::Thought],
    })
}

#[cfg(test)]
mod tests;
