use std::sync::Arc;

use serde::Deserialize;

use crate::core::EventLog;
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::store::read_event;

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

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        Ok(ToolOutput::text(read_event(
            self.log.as_ref(),
            &args.id,
            args.start,
            args.end,
        )?))
    }
}

#[cfg(test)]
mod tests;
