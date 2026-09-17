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

/// A moment as a reader types it: an ISO-8601 timestamp, or a relative
/// shorthand - `<N>d`, `<N>h`, `<N>m` - measured back from now. `None`
/// when it is neither. The grammar lives here, once; how a refusal reads
/// belongs to the surface the value was typed into, since a CLI flag and
/// a query parameter name the same value differently.
pub fn parse_time(s: &str) -> Option<Timestamp> {
    match relative_minutes(s) {
        Some(minutes) => Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    }
}

/// `<N>d`, `<N>h`, or `<N>m` as a count of minutes. `None` for anything
/// else - `parse_time` then tries it as ISO-8601.
fn relative_minutes(s: &str) -> Option<i64> {
    let (digits, unit) = s.split_at_checked(s.len().checked_sub(1)?)?;
    let n: i64 = digits.parse().ok()?;

    match unit {
        "d" => n.checked_mul(24 * 60),
        "h" => n.checked_mul(60),
        "m" => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
