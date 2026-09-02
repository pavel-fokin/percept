use std::fmt;

/// Why a `store::Event` couldn't become a domain `percept::Event`, or why
/// the JSONL log couldn't be read or written. An unknown `type` or
/// `actor` means the log was written by a newer build - the wire event
/// still deserializes, it just has no domain form here.
#[derive(Debug)]
pub enum Error {
    UnknownEventType(String),
    UnknownActor(String),
    BadUuid(String),
    BadTimestamp(String),
    BadPayload(serde_json::Error),
    /// A payload carried fields the event type doesn't record, so
    /// storing it would silently drop them.
    UnrecordedPayloadFields(String),
    /// A range was asked for on an event whose payload carries no
    /// `content` - `tool.called`, whose payload is `{tool, arguments}`.
    NoRangeableContent(String),
    /// `start` was past the content's length.
    RangeStartPastEnd {
        start: usize,
        len: usize,
    },
    /// `start` was after `end`, so the range can hold nothing.
    InvertedRange {
        start: usize,
        end: usize,
    },
    /// A log line isn't valid JSON at all - distinct from `BadPayload`,
    /// which is a well-formed line whose `payload` field doesn't match
    /// its `type`.
    BadLine(serde_json::Error),
    Io(std::io::Error),
    /// Wraps any of the above with the 1-based line number it came from.
    AtLine {
        line: usize,
        source: Box<Error>,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventType(t) => write!(
                f,
                "unknown event type: {t}, expected one of: {}",
                super::event::KINDS.join(", ")
            ),
            Self::UnknownActor(a) => write!(f, "unknown actor: {a}"),
            Self::BadUuid(s) => write!(f, "malformed uuid: {s}"),
            Self::BadTimestamp(s) => write!(f, "malformed timestamp: {s}"),
            Self::BadPayload(e) => write!(f, "malformed payload: {e}"),
            Self::UnrecordedPayloadFields(t) => {
                write!(f, "payload has fields {t} does not record")
            }
            Self::NoRangeableContent(kind) => write!(f, "{kind} has no content to slice"),
            Self::RangeStartPastEnd { start, len } => write!(
                f,
                "start {start} is past the end of content ({len} characters)"
            ),
            Self::InvertedRange { start, end } => {
                write!(f, "start {start} is not before end {end}")
            }
            Self::BadLine(e) => write!(f, "malformed line: {e}"),
            Self::Io(e) => write!(f, "{e}"),
            Self::AtLine { line, source } => write!(f, "line {line}: {source}"),
        }
    }
}

impl std::error::Error for Error {}
