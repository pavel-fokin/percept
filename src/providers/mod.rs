//! Concrete `percept::Model` implementations.
//!
//! What both wire formats share lives here: ollama's `/api/chat`
//! mirrors OpenAI's tool definition, and both stream one JSON object
//! per line.

mod ollama;
mod openai;

use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::percept::ToolSpec;

pub use ollama::Ollama;
pub use openai::OpenAi;

/// How long to wait for the server to accept a connection. Without it
/// a host that never answers hangs on the OS TCP timeout, and the reply
/// neither arrives nor fails. Only the connect is bounded. A first
/// token can be minutes away while a model loads, so a read timeout
/// would abort healthy replies.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .expect("a client with a bundled TLS backend always builds")
}

/// One tool as OpenAI's chat API defines it and ollama's copies.
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
}
