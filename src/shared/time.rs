use std::fmt;
use std::str::FromStr;

/// An instant on the system clock, UTC. Wraps `jiff::Timestamp` so the
/// rest of the code depends on this type, not the crate. `Display` and
/// `FromStr` are RFC 3339 in UTC (`...Z`), the form the wire format uses.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(jiff::Timestamp);

impl Timestamp {
    pub fn now() -> Self {
        Self(jiff::Timestamp::now())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Timestamp {
    type Err = jiff::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(Self)
    }
}
