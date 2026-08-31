//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events list` surveys the log, and
//! `percept events show` dereferences one event by id. A
//! presentation-layer peer of `tui` - it forwards parsed input to
//! `store` and has no chat logic of its own.
//!
//! `list` and `show` are the query primitive a model composes with:
//! every line is JSONL, for a caller piping into `jq`, never a table or
//! prose. `list`'s default line is constant-size per event - `preview`
//! truncates the payload - so a caller spends tokens on the full
//! payload deliberately, via `--full` or `show`.

use clap::{Args, Parser, Subcommand};
use jiff::{Span, Timestamp as JiffTimestamp};

use crate::percept::{self, EventLog};
use crate::shared::Timestamp;
use crate::store;

/// A payload's compact serialization is truncated past this many
/// characters, so `list`'s default line stays constant-size.
const PREVIEW_MAX_CHARS: usize = 120;

#[derive(Parser)]
#[command(name = "percept")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Work with the event log directly, bypassing the TUI.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Append one event to the log.
    Publish(PublishArgs),
    /// List events, one JSON object per line, oldest first.
    List(ListArgs),
    /// Print one event by id.
    Show(ShowArgs),
}

#[derive(Args)]
pub struct PublishArgs {
    #[arg(long)]
    actor: String,
    #[arg(long, value_parser = non_blank)]
    source: String,
    #[arg(long = "type")]
    kind: String,
    #[arg(long)]
    payload: String,
}

#[derive(Args)]
pub struct ListArgs {
    /// An ISO-8601 timestamp, or a relative shorthand: `<N>d`, `<N>h`,
    /// `<N>m`.
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    source: Option<String>,
    #[arg(long)]
    actor: Option<String>,
    #[arg(long = "type")]
    kind: Option<String>,
    /// Keep only the N most recent matching events. Output still runs
    /// oldest first.
    #[arg(long)]
    limit: Option<usize>,
    /// Print the whole wire event per line instead of the constant-size
    /// default.
    #[arg(long)]
    full: bool,
}

#[derive(Args)]
pub struct ShowArgs {
    id: String,
}

/// Every event must name a writer. An empty string looks deliberate to
/// a reader while naming nobody, so it is rejected at parse time.
fn non_blank(s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        return Err("must not be blank".to_string());
    }
    Ok(s.to_string())
}

/// Appends one event built from `args` to `log`. `store` owns the
/// decode, so the CLI only parses flags.
pub fn publish(args: PublishArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
    let event = store::decode(&args.actor, args.source, &args.kind, payload)?;
    log.append(&event)
}

/// Lists events matching `args`'s filters, one JSON object per line in
/// log order. `store` owns the wire shape; the CLI only filters and
/// formats it.
pub fn list(args: ListArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let events = filter(log.load()?, &args)?;

    for event in &events {
        let line = if args.full {
            store::encode(event)
        } else {
            store::summarize(event, PREVIEW_MAX_CHARS)
        };
        println!("{line}");
    }
    Ok(())
}

/// Applies `args`'s filters to `events`, converting to the wire shape
/// along the way. `--limit N` keeps the N most recent matches but
/// leaves them in log order - it slices the tail rather than sorting.
fn filter(events: Vec<percept::Event>, args: &ListArgs) -> Result<Vec<percept::Event>, String> {
    let since = args.since.as_deref().map(parse_since).transpose()?;
    let mut events = events;

    if let Some(since) = since {
        events.retain(|event| event.created_at() >= since);
    }
    if let Some(actor) = &args.actor {
        events.retain(|event| store::actor_name(event.actor()) == actor);
    }
    if let Some(source) = &args.source {
        events.retain(|event| event.source() == source);
    }
    if let Some(kind) = &args.kind {
        events.retain(|event| store::kind(event) == kind);
    }
    if let Some(limit) = args.limit {
        let start = events.len().saturating_sub(limit);
        events = events.split_off(start);
    }
    Ok(events)
}

/// Prints one event whose id matches `args.id` as the whole wire event.
/// No id in the log names it, so the search fails loudly rather than
/// printing nothing.
pub fn show(args: ShowArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let event = log
        .load()?
        .into_iter()
        .find(|event| event.id().as_uuid().to_string() == args.id)
        .ok_or_else(|| format!("no event with id {}", args.id))?;

    println!("{}", store::encode(&event));
    Ok(())
}

/// Parses `--since`: an ISO-8601 timestamp, or a relative shorthand -
/// `<N>d`, `<N>h`, `<N>m` - measured back from now.
fn parse_since(s: &str) -> Result<Timestamp, String> {
    match relative_span(s) {
        Some(span) => JiffTimestamp::now()
            .checked_sub(span)
            .map_err(|e| format!("invalid --since value {s}: {e}"))?
            .to_string()
            .parse()
            .map_err(|e| format!("invalid --since value {s}: {e}")),
        None => s
            .parse()
            .map_err(|e| format!("invalid --since value {s}: {e}")),
    }
}

/// `<N>d`, `<N>h`, or `<N>m` as a span, expressed in hours and minutes
/// so it never carries a calendar unit `Timestamp` arithmetic rejects.
/// `None` for anything else - `parse_since` then tries it as ISO-8601.
fn relative_span(s: &str) -> Option<Span> {
    let split_at = s.len().checked_sub(1)?;
    let (digits, unit) = s.split_at(split_at);
    let n: i64 = digits.parse().ok()?;

    match unit {
        "d" => n
            .checked_mul(24)
            .and_then(|hours| Span::new().try_hours(hours).ok()),
        "h" => Span::new().try_hours(n).ok(),
        "m" => Span::new().try_minutes(n).ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::Event;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeLog(Mutex<Vec<Event>>);

    impl EventLog for FakeLog {
        fn append(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
            self.0.lock().unwrap().push(event.clone());
            Ok(())
        }

        fn load(&self) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    fn args(actor: &str, payload: &str) -> PublishArgs {
        PublishArgs {
            actor: actor.to_string(),
            source: "claude-code".to_string(),
            kind: "message.received".to_string(),
            payload: payload.to_string(),
        }
    }

    #[test]
    fn a_valid_publish_appends_one_event_carrying_its_source() {
        let log = FakeLog::default();
        publish(args("user", r#"{"content":"hi"}"#), &log).unwrap();

        let events = log.load().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source(), "claude-code");
        assert!(events[0].actor() == percept::Actor::User);
    }

    #[test]
    fn a_payload_field_the_type_does_not_record_is_rejected() {
        let log = FakeLog::default();
        let extra = r#"{"content":"hi","meta":{"thread":42}}"#;
        assert!(publish(args("user", extra), &log).is_err());
        assert!(log.load().unwrap().is_empty());
    }

    #[test]
    fn a_rejected_event_appends_nothing() {
        let log = FakeLog::default();
        assert!(publish(args("robot", r#"{"content":"hi"}"#), &log).is_err());
        assert!(publish(args("user", "not json"), &log).is_err());
        assert!(log.load().unwrap().is_empty());
    }

    #[test]
    fn since_parses_an_iso8601_timestamp() {
        let parsed = parse_since("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(parsed.to_string(), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn since_parses_relative_shorthand_as_a_time_in_the_past() {
        let now = Timestamp::now();
        for shorthand in ["1d", "2h", "30m"] {
            let parsed = parse_since(shorthand).unwrap();
            assert!(parsed < now, "{shorthand} should parse to before now");
        }
    }

    #[test]
    fn since_rejects_an_unparseable_value() {
        assert!(parse_since("not a time").is_err());
    }

    fn event_at(source: &str, offset_minutes: i64) -> Event {
        let created_at = JiffTimestamp::now()
            .checked_sub(Span::new().try_minutes(offset_minutes).unwrap())
            .unwrap()
            .to_string()
            .parse()
            .unwrap();
        Event::restore(
            percept::EventId::new(),
            percept::Actor::User,
            source.to_string(),
            None,
            created_at,
            percept::Payload::MessageReceived {
                content: "hi".to_string(),
            },
        )
    }

    fn no_filters() -> ListArgs {
        ListArgs {
            since: None,
            source: None,
            actor: None,
            kind: None,
            limit: None,
            full: false,
        }
    }

    #[test]
    fn limit_keeps_the_most_recent_events_but_preserves_log_order() {
        // In log order, oldest first - `filter` trusts that order
        // rather than re-sorting by `created_at`.
        let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

        let kept = filter(
            events,
            &ListArgs {
                limit: Some(2),
                ..no_filters()
            },
        )
        .unwrap();

        let sources: Vec<_> = kept.iter().map(|e| e.source().to_string()).collect();
        assert_eq!(sources, vec!["b", "c"]);
    }

    #[test]
    fn limit_larger_than_the_log_keeps_everything() {
        let events = vec![event_at("a", 2), event_at("b", 1)];

        let kept = filter(
            events,
            &ListArgs {
                limit: Some(10),
                ..no_filters()
            },
        )
        .unwrap();

        assert_eq!(kept.len(), 2);
    }
}
