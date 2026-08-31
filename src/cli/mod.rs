//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI. A presentation-layer peer of `tui` - it
//! forwards parsed input to `store` and has no chat logic of its own.

use clap::{Args, Parser, Subcommand};
use uuid::Uuid;

use crate::percept::{self, EventLog};
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

/// Every event must name a writer. An empty string looks deliberate to
/// a reader while naming nobody, so it is rejected at parse time.
fn non_blank(s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        return Err("must not be blank".to_string());
    }
    Ok(s.to_string())
}

/// Appends one event built from `args` to `log`. Reuses `store`'s
/// wire-to-domain conversion to validate `actor`, `type`, and
/// `payload`, so that contract stays in one place; only `id` and
/// `created_at` are minted here.
pub fn publish(args: PublishArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let payload: serde_json::Value =
        serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
    let wire = store::Event {
        id: Uuid::now_v7().to_string(),
        actor: args.actor,
        source: Some(args.source),
        kind: args.kind,
        causation_id: None,
        created_at: Timestamp::now().to_string(),
        payload: payload.clone(),
    };
    let event = percept::Event::try_from(wire)?;

    // The load path drops unknown payload fields on purpose, so old
    // logs survive a format change. On the way in that same tolerance
    // would accept data the log never records, and report success.
    if store::Event::from(&event).payload != payload {
        return Err(format!("payload has fields {} does not record", event_kind(&event)).into());
    }
    log.append(&event)
}

/// The wire `type` an event serializes as - named here only to make the
/// rejection above say which shape the caller missed.
fn event_kind(event: &percept::Event) -> String {
    store::Event::from(event).kind
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
}
