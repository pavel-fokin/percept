use std::sync::Arc;

use serde::Deserialize;

use crate::core::{MapReader, NodeRef, Selection};
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::mapstore::{encode_fragment, encode_lines, encode_schema, NodeRefArgs};
use crate::store::optional_time;

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
    kinds. The maps are the ones the catalogue lists; `code`, the map \
    of files and imports, is walked fresh from the working tree. \
    Prefer this over grep for code structure. Returns JSONL: first a \
    line naming the map's node and edge kinds, each with one line on \
    what it is - read it before choosing an `around` selector, since a \
    kind's name alone can mislead. Then a line counting what was shown \
    of the whole and how many edges cross the cut, then every node, \
    then every edge, each with the event ids it cites. A crossing edge \
    is where to widen when an exception or a contradiction could \
    change the answer. Open a map before answering from it or revising \
    it; what the conversation shows of a map may be only its \
    headlines. `since` has no meaning for a derived map, which has no \
    history.";

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
        // Resolved against this same fold, so `select` below cuts the
        // map this node was found in, not a second one read after it.
        // An empty map has nothing to resolve against - `select`'s own
        // empty-map case would skip `around` anyway, so a node named on
        // one is not an error to report over "nothing recorded yet".
        let around = if map.nodes().is_empty() {
            None
        } else {
            args.around
                .map(|node| -> Result<NodeRef, crate::core::MapError> {
                    let id = node.resolve(&map)?;
                    let node = map.node(id).expect("resolve returns a live node's id");
                    Ok(NodeRef {
                        kind: node.kind.clone(),
                        name: node.name.clone(),
                    })
                })
                .transpose()?
        };
        let selection = Selection {
            around: around.as_ref().map(|node| (node, args.depth)),
            since: optional_time(args.since.as_deref())?,
            kinds: &args.kinds,
        };
        // `select` refuses `since` on the code map - it has no history.
        let fragment = map.select(&selection)?;
        let lines = [
            encode_schema(fragment.map().schema()),
            encode_fragment(&fragment),
        ]
        .into_iter()
        .chain(encode_lines(fragment.map()));
        Ok(ToolOutput::text(lines.collect::<Vec<_>>().join("\n")))
    }
}

#[cfg(test)]
mod tests;
