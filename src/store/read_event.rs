use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, Tool, ToolSpec};
use crate::store::{encode, excerpt, parse_event_id};

/// One event by its wire id, as `encode` prints it, or with `content`
/// sliced to `start..end` when either bound is given - the one path
/// both the tool and `events show` take, so an unknown id or a range
/// reads the same from a shell and from the model.
pub fn read(
    log: &dyn EventLog,
    id: &str,
    start: Option<usize>,
    end: Option<usize>,
) -> Result<String, Box<dyn std::error::Error>> {
    let event = log
        .get(parse_event_id(id)?)?
        .ok_or_else(|| format!("no event with id {id}"))?;
    if start.is_none() && end.is_none() {
        Ok(encode(&event))
    } else {
        Ok(excerpt(&event, start, end)?)
    }
}

/// The `read_event` tool: fetches one event by id and prints it as
/// `events show` does. With `start` and/or `end`, it returns
/// `payload.content` sliced to that character range instead - the
/// model's way to read past a search result's cut preview without
/// pulling the whole log into its window.
pub struct ReadEvent {
    log: Arc<dyn EventLog>,
}

impl ReadEvent {
    pub fn new(log: Arc<dyn EventLog>) -> Self {
        Self { log }
    }
}

const NAME: &str = "read_event";

const DESCRIPTION: &str = "Read one event by id, the same JSON line \
    `events show` prints. Give `start` and/or `end` - a character range \
    into `payload.content`, `end` exclusive - to read a slice instead \
    of the whole event; the result then carries `preview.len`, the \
    whole content's length. Only event kinds that carry `content` \
    support a range; `tool.called` does not.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "id": {"type": "string", "description": "the event's id"},
    "start": {"type": "integer", "description": "first character to include, 0-based; defaults to 0"},
    "end": {"type": "integer", "description": "character to stop before; defaults to the content's length"}
  },
  "required": ["id"],
  "additionalProperties": false
}"#;

/// Unknown keys are refused rather than ignored: a misspelt bound would
/// otherwise return the whole event, the very cost a range avoids. A
/// missing `id` is serde's own error, since only the bounds are optional.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    id: String,
    start: Option<usize>,
    end: Option<usize>,
}

impl Tool for ReadEvent {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<String, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        read(self.log.as_ref(), &args.id, args.start, args.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Event, EventId, Payload};
    use crate::shared::Timestamp;
    use crate::testing::FakeLog;

    fn message(content: &str) -> Event {
        Event::message_received(Actor::User, content.to_string(), "tui".to_string(), None)
    }

    #[test]
    fn spec_names_the_tool_and_carries_valid_schema_json() {
        let tool = ReadEvent::new(Arc::new(FakeLog::default()));
        let spec = tool.spec();
        assert_eq!(spec.name, "read_event");
        let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn missing_id_is_an_error() {
        let tool = ReadEvent::new(Arc::new(FakeLog::default()));
        assert!(tool.run("{}").is_err());
    }

    #[test]
    fn an_unknown_id_is_an_error() {
        let tool = ReadEvent::new(Arc::new(FakeLog::default()));
        let unknown = EventId::new().as_uuid().to_string();
        assert!(tool.run(&format!(r#"{{"id":"{unknown}"}}"#)).is_err());
    }

    #[test]
    fn no_range_returns_the_whole_event_as_show_prints_it() {
        let event = message("hello world");
        let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
        let tool = ReadEvent::new(log);

        let out = tool
            .run(&format!(r#"{{"id":"{}"}}"#, event.id().as_uuid()))
            .unwrap();
        assert_eq!(out, encode(&event));
    }

    #[test]
    fn an_end_past_the_length_clamps_to_it() {
        let event = message("hi");
        let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
        let tool = ReadEvent::new(log);

        let out = tool
            .run(&format!(
                r#"{{"id":"{}","end":9000}}"#,
                event.id().as_uuid()
            ))
            .unwrap();
        let line: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(line["payload"]["content"], "hi");
        assert_eq!(line["preview"]["len"], 2);
    }

    #[test]
    fn an_unknown_argument_is_an_error() {
        let event = message("hello");
        let tool = ReadEvent::new(Arc::new(FakeLog::seeded(vec![event.clone()])));
        let err = tool
            .run(&format!(
                r#"{{"id":"{}","range":"0:2"}}"#,
                event.id().as_uuid()
            ))
            .unwrap_err()
            .to_string();
        assert!(err.contains("range"), "{err}");
    }

    #[test]
    fn a_start_past_the_end_is_an_error() {
        let event = message("hello");
        let log = Arc::new(FakeLog::seeded(vec![event.clone()]));
        let tool = ReadEvent::new(log);

        assert!(tool
            .run(&format!(
                r#"{{"id":"{}","start":9000}}"#,
                event.id().as_uuid()
            ))
            .is_err());
    }

    #[test]
    fn a_range_on_a_tool_called_event_is_an_error() {
        let call = Event::restore(
            EventId::new(),
            Actor::Model,
            "tui".to_string(),
            None,
            Timestamp::now(),
            Payload::ToolCalled {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            },
        );
        let log = Arc::new(FakeLog::seeded(vec![call.clone()]));
        let tool = ReadEvent::new(log);

        assert!(tool
            .run(&format!(r#"{{"id":"{}","start":0}}"#, call.id().as_uuid()))
            .is_err());
    }
}
