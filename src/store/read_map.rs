use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, NodeRef, Scope, Tool, ToolOutput, ToolSpec};
use crate::store::{encode_edge, encode_node, fold_map, MapView};

/// The `read_map` tool: a current fragment with evidence IDs and an
/// explicit account of what the selection leaves out.
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

const DESCRIPTION: &str = "Read a cognitive map or a relevant fragment. \
    Returns JSONL: coverage metadata, nodes, then edges with source event IDs. \
    Use around with kind and name, depth (default 1), and optional kinds. \
    Use kinds [\"commitment\"] to discover decision overview entries before expanding one. \
    Traversal follows edges both ways before filtering kinds. Boundary counts \
    warn of omitted relationships, not their relevance. Maps are interpretations. \
    Read cited events before resolving contradictions or making consequential \
    corrections; expand the fragment to inspect affected conclusions.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "map": {"type": "string", "description": "the map's name, e.g. decisions"},
    "around": {
      "type": "object",
      "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
      "required": ["kind", "name"],
      "additionalProperties": false
    },
    "depth": {"type": "integer", "minimum": 0, "description": "edge distance, default 1; requires around"},
    "kinds": {"type": "array", "items": {"type": "string"}}
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
            return Err("depth requires around".into());
        }
        let map = fold_map(self.log.as_ref(), &args.map, &self.scope)?;
        let around = args.around.map(|node| NodeRef {
            kind: node.kind,
            name: node.name,
        });
        let view = MapView::select(map, around.as_ref(), args.depth.unwrap_or(1), &args.kinds)?;
        let mut lines = vec![view.metadata()];
        lines.extend(view.map.nodes().iter().map(encode_node));
        lines.extend(
            view.map
                .edges()
                .iter()
                .map(|edge| encode_edge(&view.map, edge)),
        );
        Ok(ToolOutput::text(lines.join("\n")))
    }
}

#[cfg(test)]
mod tests;
