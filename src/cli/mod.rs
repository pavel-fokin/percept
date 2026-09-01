//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events search` queries the log, and
//! `percept events show` dereferences one event by id. A
//! presentation-layer peer of `tui` - it forwards parsed input to
//! `store` and has no chat logic of its own.
//!
//! `search` and `show` are the query primitive a model composes with:
//! every line is JSONL, for a caller piping into `jq`, never a table or
//! prose. `search`'s default line shortens long strings in the payload,
//! so a caller spends tokens on the whole of one deliberately, via
//! `--full` or `show`.

use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};

use crate::percept::{EventId, EventLog, EventQuery, EventSearch};
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
    /// Search events, one JSON object per line, oldest first.
    Search(SearchArgs),
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

#[derive(Args, Default)]
pub struct SearchArgs {
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Inclusive.
    #[arg(long)]
    since: Option<String>,
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Exclusive.
    #[arg(long)]
    until: Option<String>,
    /// Repeatable. An event matching any of these sources passes.
    #[arg(long)]
    source: Vec<String>,
    /// Repeatable. An event matching any of these actors passes.
    #[arg(long)]
    actor: Vec<String>,
    /// Repeatable. An event matching any of these types passes.
    #[arg(long = "type")]
    kind: Vec<String>,
    /// Keep only the N most recent matching events. Output still runs
    /// oldest first.
    #[arg(long)]
    size: Option<usize>,
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

/// Searches `log` for events matching `args`, printing one JSON object
/// per line in log order. `store` owns the wire shape; the CLI only
/// builds the query and formats the result.
pub fn search(args: SearchArgs, log: &dyn EventSearch) -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query(&args)?;
    let events = log.search(&query)?;

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

/// The `EventQuery` `args` asks for, with every flag parsed once into
/// the value it is compared against. A filter naming something the log
/// has no word for is an error here rather than a query that quietly
/// matches nothing.
fn parse_query(args: &SearchArgs) -> Result<EventQuery, String> {
    let kinds = args
        .kind
        .iter()
        .map(|kind| store::parse_kind(kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let actors = args
        .actor
        .iter()
        .map(|actor| store::parse_actor(actor).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let since = args
        .since
        .as_deref()
        .map(|s| parse_time("since", s))
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(|s| parse_time("until", s))
        .transpose()?;

    // An inverted window can never match, whatever the log holds -
    // `--since 1h --until 2h` is how "between one and two hours ago"
    // is mistyped. Rejecting it keeps an empty result meaning the log
    // has nothing, the same guarantee the filters above give.
    if let (Some(since), Some(until)) = (since, until) {
        if since >= until {
            return Err(format!("--since {since} is not before --until {until}"));
        }
    }

    Ok(EventQuery {
        since,
        until,
        actors,
        sources: args.source.clone(),
        kinds,
        size: args.size,
    })
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

/// Parses a `--since`/`--until` value: an ISO-8601 timestamp, or a
/// relative shorthand - `<N>d`, `<N>h`, `<N>m` - measured back from now.
/// `flag` names the flag the value came from, so a rejected value's
/// error says which one.
fn parse_time(flag: &str, s: &str) -> Result<Timestamp, String> {
    let parsed = match relative_minutes(s) {
        Some(minutes) => Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    };
    parsed.ok_or_else(|| format!("invalid --{flag} value {s}"))
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
mod tests {
    use super::*;
    use crate::percept::{self, Event};
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
    fn parse_time_parses_an_iso8601_timestamp() {
        let parsed = parse_time("since", "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(parsed.to_string(), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn parse_time_parses_relative_shorthand_as_a_time_in_the_past() {
        let now = Timestamp::now();
        for shorthand in ["1d", "2h", "30m"] {
            let parsed = parse_time("until", shorthand).unwrap();
            assert!(parsed < now, "{shorthand} should parse to before now");
        }
    }

    #[test]
    fn parse_time_rejects_an_unparseable_value_and_names_its_flag() {
        let err = parse_time("until", "3x").err().unwrap();
        assert_eq!(err, "invalid --until value 3x");
    }

    #[test]
    fn an_unknown_type_filter_is_rejected_rather_than_matching_nothing() {
        let args = SearchArgs {
            kind: vec!["message.recieved".to_string()],
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }

    #[test]
    fn every_flag_reaches_the_query_it_builds() {
        let args = SearchArgs {
            source: vec!["tui".to_string(), "cli".to_string()],
            actor: vec!["user".to_string()],
            kind: vec!["tool.used".to_string()],
            size: Some(3),
            since: Some("1d".to_string()),
            ..Default::default()
        };

        let query = parse_query(&args).unwrap();

        assert_eq!(query.sources, vec!["tui", "cli"]);
        assert!(query.actors == vec![percept::Actor::User]);
        assert!(query.kinds == vec![percept::EventKind::ToolUsed]);
        assert_eq!(query.size, Some(3));
        assert!(query.since.is_some() && query.until.is_none());
    }

    #[test]
    fn a_window_that_ends_before_it_starts_is_rejected() {
        let args = SearchArgs {
            since: Some("1h".to_string()),
            until: Some("2h".to_string()),
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }

    #[test]
    fn an_unknown_actor_filter_is_rejected_rather_than_matching_nothing() {
        let args = SearchArgs {
            actor: vec!["User".to_string()],
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }
}
