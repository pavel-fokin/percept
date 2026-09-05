use std::error::Error;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{client, forward, role, stream_lines, Line};
use crate::percept::{
    Chunk, Message, Modality, Model, ModelCapabilities, ModelRequest, ReplyStream, ToolSpec, Usage,
};

/// Sends and receives with a local ollama server's `/api/chat`, which
/// streams NDJSON: one JSON object per line, each carrying a token of
/// the reply.
pub struct Ollama {
    url: String,
    model: String,
    client: reqwest::Client,
}

impl Ollama {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            url: format!("{base_url}/api/chat"),
            model,
            client: client(),
        }
    }
}

/// Tokens of context asked of ollama per request. Its own default is
/// 4096, which a request carrying a map, a search result, and a
/// thinking model's reasoning overruns; the reply then stops mid-thought
/// as `done_reason: length`. Sized for that, not for the model's limit.
const CONTEXT_TOKENS: u32 = 16384;

/// Families of locally-run ollama models known to think, matched
/// against the model's name before its `:tag`. Not exhaustive - a
/// model pulled under a family this list doesn't carry yet reports
/// text-only, not a guess.
const THINKING_MODEL_FAMILIES: &[&str] = &["deepseek-r1", "qwq", "gpt-oss", "magistral"];

/// What `model` writes, judged by its family name. ollama names a pull
/// as `family:tag`, so the tag is dropped before matching.
fn output_modalities(model: &str) -> &'static [Modality] {
    let family = model
        .split(':')
        .next()
        .expect("split yields at least one item");
    if THINKING_MODEL_FAMILIES.contains(&family) {
        &[Modality::Text, Modality::Thought]
    } else {
        &[Modality::Text]
    }
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    /// Omitted when empty, so a plain chat request is unchanged.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<ToolDef>,
    stream: bool,
    options: ChatOptions,
}

#[derive(Serialize)]
struct ChatOptions {
    num_ctx: u32,
}

/// One tool in ollama's shape, a copy of OpenAI's chat completions.
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
    /// A `Message::ToolCall` carries one; every other message none, so
    /// a plain turn serializes as `{ role, content }` exactly as before.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<ToolCall>,
}

/// One tool call, the same shape ollama sends in a streamed message
/// and accepts back in a replayed one.
#[derive(Serialize, Deserialize)]
struct ToolCall {
    function: ToolCallFunction,
}

#[derive(Serialize, Deserialize)]
struct ToolCallFunction {
    name: String,
    /// ollama takes and sends arguments as a JSON object; the domain
    /// carries them as text.
    arguments: Value,
}

/// One domain `Message` in ollama's `/api/chat` shape: a plain turn,
/// the model's tool call as an `assistant` message with `tool_calls`,
/// or a tool's output as a `tool` message.
fn chat_message(message: &Message) -> ChatMessage {
    match message {
        Message::Text {
            role: actor,
            content,
        } => ChatMessage {
            role: role(*actor),
            content: content.clone(),
            tool_calls: Vec::new(),
        },
        Message::ToolCall { tool, arguments } => ChatMessage {
            role: "assistant",
            content: String::new(),
            tool_calls: vec![ToolCall {
                function: ToolCallFunction {
                    name: tool.clone(),
                    arguments: serde_json::from_str(arguments)
                        .expect("ToolCall arguments is validated JSON"),
                },
            }],
        },
        Message::ToolResult { content } => ChatMessage {
            role: "tool",
            content: content.clone(),
            tool_calls: Vec::new(),
        },
    }
}

/// One line of `/api/chat`'s NDJSON body. Only the fields Ollama reads
/// are deserialized - the rest of what ollama sends (`model`,
/// `created_at`, timing stats on the final line) is ignored.
#[derive(Deserialize)]
struct ChatChunk {
    /// Absent on a line that carries `error` instead. Required here
    /// would fail the parse before the error could be read, hiding what
    /// the server actually said.
    #[serde(default)]
    message: ChatChunkMessage,
    #[serde(default)]
    done: bool,
    /// Why a `done` line ended the stream: `stop` when the model
    /// finished, `length` when the context ran out first.
    #[serde(default)]
    done_reason: String,
    #[serde(default)]
    error: Option<String>,
    /// Tokens the request carried in, on the `done` line only.
    #[serde(default)]
    prompt_eval_count: u64,
    /// Tokens the reply carried out, on the `done` line only.
    #[serde(default)]
    eval_count: u64,
}

#[derive(Deserialize, Default)]
struct ChatChunkMessage {
    #[serde(default)]
    content: String,
    /// A thinking model's reasoning, on a line of its own before its
    /// `content` - present but empty on a line that carries none.
    #[serde(default)]
    thinking: String,
    /// A tool call arrives whole on one line, not token by token.
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
}

/// The `done` line's `content` is always empty, and a line with
/// neither `thinking` nor `content` yields no chunk.
fn parse_line(line: &str, model: &str) -> Result<Line, Box<dyn Error + Send + Sync>> {
    if line.trim().is_empty() {
        return Ok(Line::Empty);
    }
    let raw: ChatChunk =
        serde_json::from_str(line).map_err(|err| format!("malformed line from ollama: {err}"))?;
    if let Some(error) = raw.error {
        return Err(format!("ollama reported: {error}").into());
    }
    if raw.done {
        // A reply cut at the context limit would otherwise pass for a
        // short one - or, after a long thought, for no reply at all.
        if raw.done_reason == "length" {
            return Err("ollama cut the reply off at its context limit".into());
        }
        return Ok(Line::Done(Usage {
            model: model.to_string(),
            input_tokens: raw.prompt_eval_count,
            output_tokens: raw.eval_count,
            cached_tokens: None,
        }));
    }
    // Thinking arrives first when a line ever carried both.
    if !raw.message.thinking.is_empty() {
        Ok(Line::Chunk(Chunk::Thought(raw.message.thinking)))
    } else if let Some(call) = raw.message.tool_calls.into_iter().next() {
        // One call per line handled - the loop runs tools one at a
        // time. A model that emits several in one message gets only
        // the first run; the log records just that, so its replayed
        // history stays consistent.
        Ok(Line::Chunk(Chunk::ToolCall {
            tool: call.function.name,
            arguments: call.function.arguments.to_string(),
        }))
    } else if !raw.message.content.is_empty() {
        Ok(Line::Chunk(Chunk::Reply(raw.message.content)))
    } else {
        Ok(Line::Empty)
    }
}

impl Model for Ollama {
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            input: &[Modality::Text],
            output: output_modalities(&self.model),
            tool_use: true,
            context_window: Some(CONTEXT_TOKENS),
        }
    }

    fn name(&self) -> &str {
        &self.model
    }

    fn reply(&self, request: &ModelRequest) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let client = self.client.clone();
        let url = self.url.clone();
        let model = self.model.clone();
        let request = ChatRequest {
            model: self.model.clone(),
            messages: request.messages.iter().map(chat_message).collect(),
            tools: request.tools.iter().map(tool_def).collect(),
            stream: true,
            options: ChatOptions {
                num_ctx: CONTEXT_TOKENS,
            },
        };

        tokio::spawn(async move {
            let request = client.post(&url).json(&request);
            let mut pending = None;
            stream_lines(request, "ollama", &tx, |line| {
                forward(parse_line(line, &model), &tx, &mut pending)
            })
            .await;
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_content_line_parses_as_a_reply_chunk() {
        let line =
            r#"{"model":"gemma4","message":{"role":"assistant","content":"Hi"},"done":false}"#;
        match parse_line(line, "gemma4").unwrap() {
            Line::Chunk(Chunk::Reply(content)) => assert_eq!(content, "Hi"),
            _ => panic!("expected a reply chunk"),
        }
    }

    #[test]
    fn a_thinking_line_parses_as_a_thought_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":"","thinking":"Thinking"},"done":false}"#;
        match parse_line(line, "gemma4").unwrap() {
            Line::Chunk(Chunk::Thought(thinking)) => assert_eq!(thinking, "Thinking"),
            _ => panic!("expected a thought chunk"),
        }
    }

    #[test]
    fn a_tool_call_line_parses_as_a_tool_call_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"search_events","arguments":{"size":5}}}]},"done":false}"#;
        match parse_line(line, "gemma4").unwrap() {
            Line::Chunk(Chunk::ToolCall { tool, arguments }) => {
                assert_eq!(tool, "search_events");
                let args: Value = serde_json::from_str(&arguments).unwrap();
                assert_eq!(args["size"], 5);
            }
            _ => panic!("expected a tool call chunk"),
        }
    }

    #[test]
    fn a_line_with_neither_thinking_nor_content_yields_no_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":false}"#;
        match parse_line(line, "gemma4").unwrap() {
            Line::Empty => {}
            _ => panic!("expected no chunk"),
        }
    }

    #[test]
    fn a_done_line_ends_the_stream_carrying_its_token_counts() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":12,"eval_count":34}"#;
        match parse_line(line, "gemma4").unwrap() {
            Line::Done(usage) => {
                assert_eq!(usage.model, "gemma4");
                assert_eq!(usage.input_tokens, 12);
                assert_eq!(usage.output_tokens, 34);
                assert_eq!(usage.cached_tokens, None);
            }
            _ => panic!("expected done with counts"),
        }
    }

    #[test]
    fn a_done_line_cut_at_the_context_limit_is_an_error() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":true,"done_reason":"length"}"#;
        let err = parse_line(line, "gemma4").err().unwrap();
        assert!(err.to_string().contains("context limit"), "{err}");
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        assert!(parse_line("not json", "gemma4").is_err());
    }

    #[test]
    fn an_error_line_surfaces_what_the_server_said() {
        let Err(err) = parse_line(r#"{"error":"model 'nope' not found"}"#, "gemma4") else {
            panic!("expected an error")
        };
        assert!(err.to_string().contains("model 'nope' not found"));
    }

    #[test]
    fn a_known_thinking_family_reports_thought_output_regardless_of_tag() {
        let ollama = Ollama::new(
            "http://localhost:11434".to_string(),
            "deepseek-r1:14b".to_string(),
        );
        assert!(ollama.capabilities().output.contains(&Modality::Thought));
    }

    #[test]
    fn an_unrecognized_family_is_text_only() {
        let ollama = Ollama::new("http://localhost:11434".to_string(), "llama3.1".to_string());
        assert_eq!(ollama.capabilities().output, &[Modality::Text]);
    }

    #[test]
    fn a_blank_line_is_skipped_rather_than_ending_the_stream() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut pending = None;
        assert!(!forward(parse_line("   ", "gemma4"), &tx, &mut pending));
        assert!(rx.try_recv().is_err());
    }
}
