use std::error::Error;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use crate::percept::{Actor, Chunk, Message, Modality, Model, ModelCapabilities, ReplyStream};

/// How long to wait for the server to accept a connection. Without it
/// a host that never answers hangs on the OS TCP timeout, and the reply
/// neither arrives nor fails.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

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
            // Only the connect is bounded. A first token can be minutes
            // away while ollama loads the model, so a read timeout
            // would abort healthy replies.
            client: reqwest::Client::builder()
                .connect_timeout(CONNECT_TIMEOUT)
                .build()
                .expect("a client with no TLS backend always builds"),
        }
    }
}

/// ollama's role vocabulary - a string, not an enum, because it's the
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
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
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
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChatChunkMessage {
    #[serde(default)]
    content: String,
    /// A thinking model's reasoning, on a line of its own before its
    /// `content` - present but empty on a line that carries none.
    #[serde(default)]
    thinking: String,
}

/// What one parsed NDJSON line means for the stream: a chunk to
/// forward, the sentinel that ends it, or nothing - the `done` line's
/// `content` is always empty, and a line with neither `thinking` nor
/// `content` yields no chunk.
enum Line {
    Chunk(Chunk),
    Empty,
    Done,
}

fn parse_line(line: &str) -> Result<Line, Box<dyn Error + Send + Sync>> {
    if line.trim().is_empty() {
        return Ok(Line::Empty);
    }
    let raw: ChatChunk =
        serde_json::from_str(line).map_err(|err| format!("malformed line from ollama: {err}"))?;
    if let Some(error) = raw.error {
        return Err(format!("ollama reported: {error}").into());
    }
    if raw.done {
        return Ok(Line::Done);
    }
    // Thinking arrives first when a line ever carried both.
    if !raw.message.thinking.is_empty() {
        Ok(Line::Chunk(Chunk::Thought(raw.message.thinking)))
    } else if !raw.message.content.is_empty() {
        Ok(Line::Chunk(Chunk::Reply(raw.message.content)))
    } else {
        Ok(Line::Empty)
    }
}

/// Splits newly arrived bytes on `\n`, returning each complete line and
/// leaving an incomplete tail in `buf` for the next call - `bytes_stream`
/// chunk boundaries don't align with NDJSON line boundaries.
fn take_lines(buf: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buf.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        lines.push(String::from_utf8_lossy(&buf[..pos]).into_owned());
        buf.drain(..=pos);
    }
    lines
}

/// Parses and forwards one line. Returns `true` once the stream is
/// over - the `done` sentinel, a parse failure, or the receiver having
/// gone away - so the caller knows to stop reading the body.
fn handle_line(
    line: &str,
    tx: &UnboundedSender<Result<Chunk, Box<dyn Error + Send + Sync>>>,
) -> bool {
    match parse_line(line) {
        Ok(Line::Chunk(chunk)) => tx.send(Ok(chunk)).is_err(),
        Ok(Line::Empty) => false,
        Ok(Line::Done) => true,
        Err(err) => {
            let _ = tx.send(Err(err));
            true
        }
    }
}

impl Model for Ollama {
    fn capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            input: &[Modality::Text],
            output: &[Modality::Text],
            tool_use: false,
        }
    }

    fn reply(&self, messages: &[Message]) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let client = self.client.clone();
        let url = self.url.clone();
        let request = ChatRequest {
            model: self.model.clone(),
            messages: messages
                .iter()
                .map(|m| ChatMessage {
                    role: role(m.role),
                    content: m.content.clone(),
                })
                .collect(),
            stream: true,
        };

        tokio::spawn(async move {
            let response = match client.post(&url).json(&request).send().await {
                Ok(response) => response,
                Err(err) => {
                    let _ = tx.send(Err(format!("request to ollama failed: {err}").into()));
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx.send(Err(format!("ollama returned {status}: {body}").into()));
                return;
            }

            let mut body = response.bytes_stream();
            let mut buf = Vec::new();
            while let Some(chunk) = body.next().await {
                let chunk = match chunk {
                    Ok(chunk) => chunk,
                    Err(err) => {
                        let _ = tx.send(Err(format!("ollama stream failed: {err}").into()));
                        return;
                    }
                };
                for line in take_lines(&mut buf, &chunk) {
                    if handle_line(&line, &tx) {
                        return;
                    }
                }
            }

            // Whatever the body ended on without a trailing newline is
            // still a line; an empty tail parses as nothing.
            handle_line(&String::from_utf8_lossy(&buf), &tx);
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_split_across_chunks_is_carried_to_completion() {
        let mut buf = Vec::new();
        assert!(take_lines(&mut buf, b"{\"foo\":1").is_empty());
        let lines = take_lines(&mut buf, b"}\n{\"bar\":2}\n");
        assert_eq!(lines, vec!["{\"foo\":1}", "{\"bar\":2}"]);
        assert!(buf.is_empty());
    }

    #[test]
    fn an_unterminated_final_line_is_left_in_the_buffer() {
        let mut buf = Vec::new();
        let lines = take_lines(&mut buf, b"{\"foo\":1}\nunterminated");
        assert_eq!(lines, vec!["{\"foo\":1}"]);
        assert_eq!(buf, b"unterminated");
    }

    #[test]
    fn a_content_line_parses_as_a_reply_chunk() {
        let line =
            r#"{"model":"gemma4","message":{"role":"assistant","content":"Hi"},"done":false}"#;
        match parse_line(line).unwrap() {
            Line::Chunk(Chunk::Reply(content)) => assert_eq!(content, "Hi"),
            _ => panic!("expected a reply chunk"),
        }
    }

    #[test]
    fn a_thinking_line_parses_as_a_thought_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":"","thinking":"Thinking"},"done":false}"#;
        match parse_line(line).unwrap() {
            Line::Chunk(Chunk::Thought(thinking)) => assert_eq!(thinking, "Thinking"),
            _ => panic!("expected a thought chunk"),
        }
    }

    #[test]
    fn a_line_with_neither_thinking_nor_content_yields_no_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":false}"#;
        match parse_line(line).unwrap() {
            Line::Empty => {}
            _ => panic!("expected no chunk"),
        }
    }

    #[test]
    fn a_done_line_ends_the_stream_without_a_trailing_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#;
        match parse_line(line).unwrap() {
            Line::Done => {}
            _ => panic!("expected done"),
        }
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        assert!(parse_line("not json").is_err());
    }

    #[test]
    fn an_error_line_surfaces_what_the_server_said() {
        let Err(err) = parse_line(r#"{"error":"model 'nope' not found"}"#) else {
            panic!("expected an error")
        };
        assert!(err.to_string().contains("model 'nope' not found"));
    }

    #[test]
    fn a_blank_line_is_skipped_rather_than_ending_the_stream() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        assert!(!handle_line("   ", &tx));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn roles_map_to_ollamas_vocabulary() {
        assert_eq!(role(Actor::User), "user");
        assert_eq!(role(Actor::Model), "assistant");
        assert_eq!(role(Actor::System), "system");
    }
}
