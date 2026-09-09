use std::sync::Arc;

use serde::Deserialize;

use crate::core::MapReader;
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::mapstore::NodeRefArgs;
use crate::store::optional_time;
use crate::tools::read_selection;

/// The `read_map` tool: one map, whole or cut to a fragment, as JSONL
/// with event ids on every node and edge. Offered when the prompt does
/// not carry the map whole, so the model opens what it judges relevant
/// instead of reading every map every turn.
pub struct ReadMap {
    maps: Arc<dyn MapReader>,
}

impl ReadMap {
    pub fn new(maps: Arc<dyn MapReader>) -> Self {
        Self { maps }
    }
}

const NAME: &str = "read_map";

const DESCRIPTION: &str = "Read one cognitive map by name, whole or cut to \
    a fragment: around one node to a depth, since an instant, of some \
    kinds. The maps are the ones the catalogue lists. Returns JSONL: \
    first a line naming the map's node and edge kinds, each with one \
    line on what it is - read it before choosing an `around` selector, \
    since a kind's name alone can mislead. Then a line counting what \
    was shown of the whole and how many edges cross the cut, then \
    every node, then every edge, each with the event ids it cites. A \
    crossing edge is where to widen when an exception or a \
    contradiction could change the answer. Open a map before answering \
    from it or revising it; what the conversation shows of a map may \
    be only its headlines.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "map": {"type": "string", "description": "the map's name, as the catalogue lists it"},
    "around": {
      "description": "keep this node and what is within depth edges of it, either way",
      "oneOf": [
        {
          "type": "object",
          "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
          "required": ["kind", "name"],
          "additionalProperties": false
        },
        {"type": "string", "description": "a short id like d41, as this map's own nodes are shown"}
      ]
    },
    "depth": {"type": "integer", "minimum": 0, "description": "edges out from around, default 1; nothing without around"},
    "since": {"type": "string", "description": "ISO-8601, or 1d/2h/30m back from now; keep what the map gained since then"},
    "kinds": {"type": "array", "items": {"type": "string"}, "description": "keep only these node kinds"}
  },
  "required": ["map"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    map: String,
    around: Option<NodeRefArgs>,
    #[serde(default = "one")]
    depth: usize,
    since: Option<String>,
    #[serde(default)]
    kinds: Vec<String>,
}

fn one() -> usize {
    1
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
        let map = self.maps.read(&args.map)?;
        read_selection(
            map,
            args.around,
            args.depth,
            optional_time(args.since.as_deref())?,
            &args.kinds,
            true,
        )
    }
}

#[cfg(test)]
mod tests;
