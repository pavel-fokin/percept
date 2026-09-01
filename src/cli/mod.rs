//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events list` surveys the log, and
//! `percept events show` dereferences one event by id. A
//! presentation-layer peer of `tui` - it forwards parsed input to
//! `store` and has no chat logic of its own.
//!
//! `list` and `show` are the query primitive a model composes with:
//! every line is JSONL, for a caller piping into `jq`, never a table or
//! prose. `list`'s default line shortens long strings in the payload,
//! so a caller spends tokens on the whole of one deliberately, via
//! `--full` or `show`.

use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};

use crate::percept::{self, Actor, EventId, EventKind, EventLog};
use crate::shared::Timestamp;
use crate::store;

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
    let events = Filters::parse(&args)?.apply(log.load()?);

    // One buffered writer rather than a syscall per line: the caller is
    // a pipe into jq, and a whole-log listing runs to thousands of
    // lines.
    let mut out = io::BufWriter::new(io::stdout().lock());
    for event in &events {
        let line = if args.full {
            store::encode(event)
        } else {
            store::summarize(event)
        };
        if let Err(e) = writeln!(out, "{line}") {
            return stop_if_pipe_closed(e);
        }
    }
    out.flush().or_else(stop_if_pipe_closed)
}

/// A reader that stops early - `head`, or a `jq` that has seen enough -
/// closes the pipe. That is the caller's choice, not a failure to
/// report.
fn stop_if_pipe_closed(e: io::Error) -> Result<(), Box<dyn std::error::Error>> {
    if e.kind() == io::ErrorKind::BrokenPipe {
        Ok(())
    } else {
        Err(e.into())
    }
}

/// The filters `args` asks for, each parsed once into the value it is
/// compared against. A filter naming something the log has no word for
/// is an error here rather than a query that quietly matches nothing.
struct Filters {
    since: Option<Timestamp>,
    actor: Option<Actor>,
    source: Option<String>,
    kind: Option<EventKind>,
    limit: Option<usize>,
}

impl Filters {
    fn parse(args: &ListArgs) -> Result<Self, String> {
        let kind = args
            .kind
            .as_deref()
            .map(|kind| {
                store::parse_kind(kind).map_err(|_| {
                    format!(
                        "unknown --type {kind}, expected one of: {}",
                        store::KINDS.join(", ")
                    )
                })
            })
            .transpose()?;

        Ok(Self {
            since: args.since.as_deref().map(parse_since).transpose()?,
            actor: args
                .actor
                .as_deref()
                .map(|a| store::parse_actor(a).map_err(|e| e.to_string()))
                .transpose()?,
            source: args.source.clone(),
            kind,
            limit: args.limit,
        })
    }

    /// `limit` keeps the most recent matches but leaves them in log
    /// order - it slices the tail rather than sorting.
    fn apply(&self, mut events: Vec<percept::Event>) -> Vec<percept::Event> {
        if let Some(since) = self.since {
            events.retain(|event| event.created_at() >= since);
        }
        if let Some(actor) = self.actor {
            events.retain(|event| event.actor() == actor);
        }
        if let Some(source) = &self.source {
            events.retain(|event| event.source() == source);
        }
        if let Some(kind) = self.kind {
            events.retain(|event| event.kind() == kind);
        }
        if let Some(limit) = self.limit {
            let start = events.len().saturating_sub(limit);
            events = events.split_off(start);
        }
        events
    }
}

/// Prints the one event `args.id` names. An id the log doesn't carry
/// fails loudly rather than printing nothing, so an empty result never
/// means "your id was wrong".
pub fn show(args: ShowArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let wanted: EventId = store::parse_event_id(&args.id)?;
    let event = log
        .get(wanted)?
        .ok_or_else(|| format!("no event with id {}", args.id))?;

    println!("{}", store::encode(&event));
    Ok(())
}

/// Parses `--since`: an ISO-8601 timestamp, or a relative shorthand -
/// `<N>d`, `<N>h`, `<N>m` - measured back from now.
fn parse_since(s: &str) -> Result<Timestamp, String> {
    let parsed = match relative_minutes(s) {
        Some(minutes) => Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    };
    parsed.ok_or_else(|| format!("invalid --since value {s}"))
}

/// `<N>d`, `<N>h`, or `<N>m` as a count of minutes. `None` for anything
/// else - `parse_since` then tries it as ISO-8601.
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

        fn get(&self, id: EventId) -> Result<Option<Event>, Box<dyn std::error::Error>> {
            let events = self.0.lock().unwrap();
            Ok(events.iter().find(|event| event.id() == id).cloned())
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

    #[test]
    fn an_unknown_type_filter_is_rejected_rather_than_matching_nothing() {
        let args = ListArgs {
            kind: Some("message.recieved".to_string()),
            ..no_filters()
        };
        assert!(Filters::parse(&args).is_err());
    }

    #[test]
    fn an_unknown_actor_filter_is_rejected_rather_than_matching_nothing() {
        let args = ListArgs {
            actor: Some("User".to_string()),
            ..no_filters()
        };
        assert!(Filters::parse(&args).is_err());
    }

    fn event_at(source: &str, offset_minutes: i64) -> Event {
        let created_at = Timestamp::now().minus_minutes(offset_minutes).unwrap();
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
        // In log order, oldest first - `apply` trusts that order
        // rather than re-sorting by `created_at`.
        let events = vec![event_at("a", 3), event_at("b", 2), event_at("c", 1)];

        let kept = Filters::parse(&ListArgs {
            limit: Some(2),
            ..no_filters()
        })
        .unwrap()
        .apply(events);

        let sources: Vec<_> = kept.iter().map(|e| e.source().to_string()).collect();
        assert_eq!(sources, vec!["b", "c"]);
    }

    #[test]
    fn limit_larger_than_the_log_keeps_everything() {
        let events = vec![event_at("a", 2), event_at("b", 1)];

        let kept = Filters::parse(&ListArgs {
            limit: Some(10),
            ..no_filters()
        })
        .unwrap()
        .apply(events);

        assert_eq!(kept.len(), 2);
    }
}
