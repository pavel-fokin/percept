use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, NodeRef, Scope, Selection, Tool, ToolOutput, ToolSpec};
use crate::store::search_events::parse_time;
use crate::store::{encode_edge, encode_fragment, encode_node, fold_map};

/// The `read_map` tool: one map, whole or cut to a fragment, as JSONL
/// with event ids on every node and edge. Offered when the prompt does
/// not carry the map whole, so the model opens what it judges relevant
/// instead of reading every map every turn.
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

const DESCRIPTION: &str = "Read one cognitive map by name, whole or cut to \
    a fragment: around one node to a depth, since an instant, of some \
    kinds. Returns JSONL: first a line counting what was shown of the \
    whole and how many edges cross the cut, then every node, then every \
    edge, each with the event ids it cites. A crossing edge is where to \
    widen when an exception or a contradiction could change the answer. \
    Open a map before answering from it or revising it; what the \
    conversation shows of a map may be only its headlines.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "map": {"type": "string", "description": "the map's name, e.g. decisions"},
    "around": {
      "type": "object",
      "description": "keep this node and what is within depth edges of it, either way",
      "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
      "required": ["kind", "name"],
      "additionalProperties": false
    },
    "depth": {"type": "integer", "minimum": 0, "description": "edges out from around, default 1"},
    "since": {"type": "string", "description": "ISO-8601; keep what the map gained since then"},
    "kinds": {"type": "array", "items": {"type": "string"}, "description": "keep only these node kinds"}
  },
  "required": ["map"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    map: String,
    around: Option<Around>,
    depth: Option<usize>,
    since: Option<String>,
    #[serde(default)]
    kinds: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Around {
    kind: String,
    name: String,
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
        if args.depth.is_some() && args.around.is_none() {
            return Err("depth needs around".into());
        }
        let around = args.around.map(|node| NodeRef {
            kind: node.kind,
            name: node.name,
        });
        let since = args.since.as_deref().map(parse_time).transpose()?;
        let selection = Selection {
            around: around.as_ref().map(|node| (node, args.depth.unwrap_or(1))),
            since,
            kinds: &args.kinds,
        };
        let fragment = fold_map(self.log.as_ref(), &args.map, &self.scope)?.select(&selection)?;
        let map = fragment.map();
        let lines = std::iter::once(encode_fragment(&fragment))
            .chain(map.nodes().iter().map(|node| encode_node(map, node)))
            .chain(map.edges().iter().map(|edge| encode_edge(map, edge)));
        Ok(ToolOutput::text(lines.collect::<Vec<_>>().join("\n")))
    }
}

#[cfg(test)]
mod tests;
