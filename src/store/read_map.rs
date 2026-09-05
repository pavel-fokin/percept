use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, Scope, Tool, ToolOutput, ToolSpec};
use crate::store::fold_map;

/// The `read_map` tool: prints one map by name, the same text the
/// prompt carries when a map is sent whole. Offered when it is not, so
/// the model can open a map it judges relevant instead of reading every
/// map every turn.
pub struct ReadMap {
    log: Arc<dyn EventLog>,
    scope: Scope,
}

impl ReadMap {
    pub fn new(log: Arc<dyn EventLog>, scope: Scope) -> Self {
        Self { log, scope }
    }
}

const NAME: &str = "read_map";

const DESCRIPTION: &str = "Read one cognitive map by name: every node, \
    then every edge, as the map stands now. Open a map before answering \
    from it or revising it; what the conversation shows of a map may be \
    only its headlines.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "map": {"type": "string", "description": "the map's name, e.g. decisions"}
  },
  "required": ["map"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    map: String,
}

impl Tool for ReadMap {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let map = fold_map(self.log.as_ref(), &args.map, &self.scope)?;
        if map.nodes().is_empty() {
            return Ok(ToolOutput::text(format!(
                "the {} map is empty: nothing has been recorded here yet. \
                 The log may still hold what it would.",
                args.map
            )));
        }
        Ok(ToolOutput::text(map.to_string()))
    }
}

#[cfg(test)]
mod tests;
