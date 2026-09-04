//! Concrete `percept::Model` implementations.
//!
//! What both wire formats share lives here: the same three role words,
//! and a reply streamed as one JSON object per line over HTTP.

mod catalog;
mod ollama;
mod openai;

use std::error::Error;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use crate::percept::{Actor, Chunk, Usage};

pub use catalog::Catalog;
pub use ollama::Ollama;
pub use openai::OpenAi;

/// How long to wait for the server to accept a connection. Without it
/// a host that never answers hangs on the OS TCP timeout, and the reply
/// neither arrives nor fails. Only the connect is bounded. A first
/// token can be minutes away while a model loads, so a read timeout
/// would abort healthy replies.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

type Sender = UnboundedSender<Result<Chunk, Box<dyn Error + Send + Sync>>>;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("a client with a bundled TLS backend always builds")
}

/// The wire's role vocabulary - a string, not an enum, because it's
/// the wire's word, not the domain's.
fn role(actor: Actor) -> &'static str {
    match actor {
        Actor::User => "user",
        Actor::Model => "assistant",
        Actor::System => "system",
    }
}

/// What one line of a streamed reply means: a chunk to forward, the
/// sentinel that ends the stream, carrying what the reply cost, or
/// nothing.
enum Line {
    Chunk(Chunk),
    Empty,
    Done(Usage),
}

/// Forwards what one line parsed to. Returns `true` once the stream is
/// over - the sentinel, a parse failure, or the receiver having gone
/// away - so the caller knows to stop reading the body. A tool call is
/// held in `pending` rather than sent straight away: its counts arrive
/// on the line that ends the stream, and `Usage` has to reach the
/// caller before the `ToolCall` that ends its turn.
fn forward(
    parsed: Result<Line, Box<dyn Error + Send + Sync>>,
    tx: &Sender,
    pending: &mut Option<Chunk>,
) -> bool {
    match parsed {
        Ok(Line::Chunk(chunk @ Chunk::ToolCall { .. })) => {
            *pending = Some(chunk);
            false
        }
        Ok(Line::Chunk(chunk)) => tx.send(Ok(chunk)).is_err(),
        Ok(Line::Empty) => false,
        Ok(Line::Done(usage)) => {
            if tx.send(Ok(Chunk::Usage(usage))).is_err() {
                return true;
            }
            if let Some(call) = pending.take() {
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

/// Sends `request` and feeds every line of the streamed body to
/// `on_line` until it returns `true` or the body ends. A failure to
/// send, a non-2xx status, or a broken body is sent to `tx` as the
/// stream's error, named after `provider`.
async fn stream_lines(
    request: reqwest::RequestBuilder,
    provider: &str,
    tx: &Sender,
    mut on_line: impl FnMut(&str) -> bool,
) {
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            let _ = tx.send(Err(format!("request to {provider} failed: {err}").into()));
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let _ = tx.send(Err(format!("{provider} returned {status}: {body}").into()));
        return;
    }

    let mut body = response.bytes_stream();
    let mut buf = Vec::new();
    while let Some(chunk) = body.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(err) => {
                let _ = tx.send(Err(format!("{provider} stream failed: {err}").into()));
                return;
            }
        };
        for line in take_lines(&mut buf, &chunk) {
            if on_line(&line) {
                return;
            }
        }
    }

    // Whatever the body ended on without a trailing newline is still
    // a line; an empty tail parses as nothing.
    on_line(&String::from_utf8_lossy(&buf));
}

/// Splits newly arrived bytes on `\n`, returning each complete line and
/// leaving an incomplete tail in `buf` for the next call - `bytes_stream`
/// chunk boundaries don't align with line boundaries.
fn take_lines(buf: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buf.extend_from_slice(chunk);
    let mut lines = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        lines.push(String::from_utf8_lossy(&buf[..pos]).into_owned());
        buf.drain(..=pos);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::usage;

    fn call() -> Chunk {
        Chunk::ToolCall {
            tool: "search_events".to_string(),
            arguments: "{}".to_string(),
        }
    }

    #[test]
    fn a_tool_call_then_done_comes_out_as_usage_then_the_call() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut pending = None;

        assert!(!forward(Ok(Line::Chunk(call())), &tx, &mut pending));
        assert!(rx.try_recv().is_err(), "the call is held, not sent yet");
        assert!(forward(Ok(Line::Done(usage())), &tx, &mut pending));

        match rx.try_recv().unwrap().unwrap() {
            Chunk::Usage(sent) => assert_eq!(sent, usage()),
            _ => panic!("expected a usage chunk"),
        }
        match rx.try_recv().unwrap().unwrap() {
            Chunk::ToolCall { tool, .. } => assert_eq!(tool, "search_events"),
            _ => panic!("expected the held tool call"),
        }
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_tool_call_then_a_failure_comes_out_as_just_the_error() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let mut pending = None;

        assert!(!forward(Ok(Line::Chunk(call())), &tx, &mut pending));
        assert!(forward(Err("upstream broke".into()), &tx, &mut pending));

        let Err(err) = rx.try_recv().unwrap() else {
            panic!("expected an error")
        };
        assert!(err.to_string().contains("upstream broke"));
        assert!(rx.try_recv().is_err(), "the held call is dropped");
    }

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
    fn roles_map_to_the_wires_vocabulary() {
        assert_eq!(role(Actor::User), "user");
        assert_eq!(role(Actor::Model), "assistant");
        assert_eq!(role(Actor::System), "system");
    }
}
