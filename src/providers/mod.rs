//! Concrete `percept::Model` implementations.
//!
//! What both wire formats share lives here: the same three role words,
//! and a reply streamed as one JSON object per line over HTTP.

mod ollama;
mod openai;

use std::error::Error;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use tokio_stream::StreamExt;

use crate::percept::{Actor, Chunk, Usage};

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
/// sentinel that ends the stream, carrying the reply's token counts,
/// or nothing.
enum Line {
    Chunk(Chunk),
    Empty,
    Done(UsageCounts),
}

/// Token counts as a provider's own wire shape reports them, before
/// `model` - which the free `parse_line` function never sees - turns
/// them into a `Usage`.
struct UsageCounts {
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: Option<u64>,
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
    model: &str,
    pending: &mut Option<Chunk>,
) -> bool {
    match parsed {
        Ok(Line::Chunk(chunk @ Chunk::ToolCall { .. })) => {
            *pending = Some(chunk);
            false
        }
        Ok(Line::Chunk(chunk)) => tx.send(Ok(chunk)).is_err(),
        Ok(Line::Empty) => false,
        Ok(Line::Done(counts)) => {
            let usage = Usage {
                model: model.to_string(),
                input_tokens: counts.input_tokens,
                output_tokens: counts.output_tokens,
                cached_tokens: counts.cached_tokens,
            };
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
