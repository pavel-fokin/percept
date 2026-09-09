use std::path::PathBuf;

use serde::Deserialize;

use crate::code;
use crate::core::{NodeRef, Selection};
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::mapstore::{encode_fragment, encode_lines, encode_schema, NodeRefArgs};

/// The `read_code` tool: the code structure - a codebase's files, the
/// symbols they define, and what imports what - walked fresh from the
/// working tree on every call, whole or cut to a fragment. Offered
/// only in the `code` toolset, beside the file tools.
pub struct ReadCode {
    root: PathBuf,
}

impl ReadCode {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

const NAME: &str = "read_code";

const DESCRIPTION: &str = "Read the code structure: which file defines which \
    symbol and imports which file or package, walked fresh from the \
    working tree on every call. Whole, or cut to a fragment: around \
    one node to a depth, of some kinds. Prefer this over grep for code \
    structure. Returns JSONL: first a line naming the map's node and \
    edge kinds, each with one line on what it is - read it before \
    choosing an `around` selector, since a kind's name alone can \
    mislead. Then a line counting what was shown of the whole and how \
    many edges cross the cut, then every node, then every edge. A \
    crossing edge is where to widen when the answer needs more than \
    what was shown.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "around": {
      "description": "keep this node and what is within depth edges of it, either way",
      "oneOf": [
        {
          "type": "object",
          "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
          "required": ["kind", "name"],
          "additionalProperties": false
        },
        {"type": "string", "description": "a short id like fn3, as this map's own nodes are shown"}
      ]
    },
    "depth": {"type": "integer", "minimum": 0, "description": "edges out from around, default 1; nothing without around"},
    "kinds": {"type": "array", "items": {"type": "string"}, "description": "keep only these node kinds"}
  },
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    around: Option<NodeRefArgs>,
    #[serde(default = "one")]
    depth: usize,
    #[serde(default)]
    kinds: Vec<String>,
}

fn one() -> usize {
    1
}

impl Tool for ReadCode {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let map = code::build(&self.root)?;
        // Resolved against this same walk, so `select` below cuts the
        // map this node was found in, not a second one built after it.
        // An empty map has nothing to resolve against - `select`'s own
        // empty-map case would skip `around` anyway, so a node named on
        // one is not an error to report over "nothing found yet".
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
            since: None,
            kinds: &args.kinds,
        };
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
