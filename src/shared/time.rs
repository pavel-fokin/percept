use std::fmt;
use std::str::FromStr;

/// An instant on the system clock, UTC. Wraps `jiff::Timestamp` so the
/// rest of the code depends on this type, not the crate. `Display` and
/// `FromStr` are RFC 3339 in UTC (`...Z`), the form the wire format uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(jiff::Timestamp);

impl Timestamp {
    pub fn now() -> Self {
        Self(jiff::Timestamp::now())
    }

    /// The instant `seconds` and `nanos` after the Unix epoch, or `None`
    /// past the range a timestamp can hold.
    pub fn from_unix(seconds: i64, nanos: i32) -> Option<Self> {
        jiff::Timestamp::new(seconds, nanos).ok().map(Self)
    }

    /// The calendar date in UTC, `2026-09-05` - what a heading wants
    /// where the full instant would be noise.
    pub fn date(&self) -> String {
        self.0.strftime("%Y-%m-%d").to_string()
    }

    /// This instant less `minutes`, or `None` if that leaves the range
    /// a timestamp can hold. Minutes, not days: a day is a calendar
    /// unit, and an instant has no calendar to measure it against.
    pub fn minus_minutes(self, minutes: i64) -> Option<Self> {
        let span = jiff::Span::new().try_minutes(minutes).ok()?;
        self.0.checked_sub(span).ok().map(Self)
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
