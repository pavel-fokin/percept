use std::fmt;

/// Why a `store::Event` couldn't become a domain `percept::Event`. An
/// unknown `type` or `actor` means the log was written by a newer build -
/// the wire event still deserializes, it just has no domain form here.
#[derive(Debug)]
pub enum Error {
    UnknownEventType(String),
    UnknownActor(String),
    BadUuid(String),
    BadTimestamp(String),
    BadPayload(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventType(t) => write!(f, "unknown event type: {t}"),
            Self::UnknownActor(a) => write!(f, "unknown actor: {a}"),
            Self::BadUuid(s) => write!(f, "malformed uuid: {s}"),
            Self::BadTimestamp(s) => write!(f, "malformed timestamp: {s}"),
            Self::BadPayload(e) => write!(f, "malformed payload: {e}"),
        }
    }
}

impl std::error::Error for Error {}
