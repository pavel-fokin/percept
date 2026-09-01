use std::error::Error;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio_stream::StreamExt;

use crate::percept::{Actor, Message, Model, ReplyStream};

/// Sends and receives with a local ollama server's `/api/chat`, which
/// streams NDJSON: one JSON object per line, each carrying a token of
/// the reply.
pub struct Ollama {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl Ollama {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            base_url,
            model,
            client: reqwest::Client::new(),
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
    message: ChatChunkMessage,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct ChatChunkMessage {
    #[serde(default)]
    content: String,
}

/// What one parsed NDJSON line means for the stream: more text to
/// append, or the sentinel that ends it. The `done` line's `content` is
/// always empty, so it never surfaces as a chunk.
enum Line {
    Content(String),
    Done,
}

fn parse_line(line: &str) -> Result<Line, serde_json::Error> {
    let chunk: ChatChunk = serde_json::from_str(line)?;
    if chunk.done {
        Ok(Line::Done)
    } else {
        Ok(Line::Content(chunk.message.content))
    }
}

/// Splits newly arrived bytes on `\n`, returning each complete line and
/// leaving an incomplete tail in `buf` for the next call - `bytes_stream`
/// chunk boundaries don't align with NDJSON line boundaries.
fn take_lines(buf: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buf.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let rest = buf.split_off(pos + 1);
        let line = std::mem::replace(buf, rest);
        lines.push(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned());
    }
    lines
}

/// Whatever is left once the body ends without a trailing newline -
/// still a line, just one the server never terminated.
fn take_final_line(buf: &[u8]) -> Option<String> {
    if buf.is_empty() {
        None
    } else {
        Some(String::from_utf8_lossy(buf).into_owned())
    }
}

/// Parses and forwards one line. Returns `true` once the stream is
/// over - the `done` sentinel, a parse failure, or the receiver having
/// gone away - so the caller knows to stop reading the body.
fn handle_line(
    line: &str,
    tx: &UnboundedSender<Result<String, Box<dyn Error + Send + Sync>>>,
) -> bool {
    match parse_line(line) {
        Ok(Line::Content(content)) => tx.send(Ok(content)).is_err(),
        Ok(Line::Done) => true,
        Err(err) => {
            let _ = tx.send(Err(format!("malformed line from ollama: {err}").into()));
            true
        }
    }
}

impl Model for Ollama {
    fn reply(&self, messages: &[Message]) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        let client = self.client.clone();
        let url = format!("{}/api/chat", self.base_url);
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

            if let Some(line) = take_final_line(&buf) {
                handle_line(&line, &tx);
            }
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
    fn an_unterminated_final_line_is_still_taken() {
        let mut buf = Vec::new();
        let lines = take_lines(&mut buf, b"{\"foo\":1}\nunterminated");
        assert_eq!(lines, vec!["{\"foo\":1}"]);
        assert_eq!(take_final_line(&buf), Some("unterminated".to_string()));
    }

    #[test]
    fn an_empty_tail_has_no_final_line() {
        let mut buf = Vec::new();
        take_lines(&mut buf, b"{\"foo\":1}\n");
        assert_eq!(take_final_line(&buf), None);
    }

    #[test]
    fn a_content_line_parses_as_content() {
        let line =
            r#"{"model":"gemma4","message":{"role":"assistant","content":"Hi"},"done":false}"#;
        match parse_line(line).unwrap() {
            Line::Content(content) => assert_eq!(content, "Hi"),
            Line::Done => panic!("expected content"),
        }
    }

    #[test]
    fn a_done_line_ends_the_stream_without_a_trailing_chunk() {
        let line = r#"{"model":"gemma4","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop"}"#;
        match parse_line(line).unwrap() {
            Line::Done => {}
            Line::Content(_) => panic!("expected done"),
        }
    }

    #[test]
    fn a_malformed_line_is_an_error() {
        assert!(parse_line("not json").is_err());
    }

    #[test]
    fn roles_map_to_ollamas_vocabulary() {
        assert_eq!(role(Actor::User), "user");
        assert_eq!(role(Actor::Model), "assistant");
        assert_eq!(role(Actor::System), "system");
    }
}
