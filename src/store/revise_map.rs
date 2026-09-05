use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, Mutation, NodeRef, Payload, Scope};
use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::store::Snapshot;

/// The `revise_map` tool: checks a batch of changes to one map against
/// its current state, in order, and hands back the payloads to commit -
/// all of them, or none. `App` commits them, caused by the call; this
/// tool never writes the log itself.
pub struct ReviseMap {
    log: Arc<dyn EventLog>,
    scope: Scope,
}

impl ReviseMap {
    pub fn new(log: Arc<dyn EventLog>, scope: Scope) -> Self {
        Self { log, scope }
    }
}

const NAME: &str = "revise_map";

const DESCRIPTION: &str = "Record into a named map what you have judged \
    from the log: a question that was raised, the options weighed, \
    evidence for or against, the decision taken. One call carries a \
    batch of changes to one map, checked together in order and \
    committed only if every change passes - a later change may refer to \
    a node an earlier one in the same batch just added. Cite the event \
    ids the judgement came from in `sources`, as search_events returns \
    them; nothing else is an id. A node with no sources is refused: a \
    map records what the log shows, so search for the events first, \
    even when what you are recording is in front of you. Read the map \
    first, from the conversation or with read_map, and do not add a \
    node that is already there; a node is named by its kind and name, \
    not by an id you choose.";

/// JSON Schema for `run`'s `arguments`. A string, not a `Value` - the
/// domain's `ToolSpec` is serde-free, so the provider parses this. The
/// node reference object - the `kind` and `name` a writer knows a node
/// by - is written out for each of `from` and `to` rather than shared
/// by a `$ref`, so no provider has to resolve one.
const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "map": {"type": "string", "description": "the map's name, e.g. decisions"},
    "changes": {
      "type": "array",
      "minItems": 1,
      "items": {
        "oneOf": [
          {
            "type": "object",
            "properties": {
              "op": {"const": "add_node"},
              "kind": {"type": "string"},
              "name": {"type": "string"},
              "properties": {"type": "object", "additionalProperties": {"type": "string"}},
              "sources": {"type": "array", "items": {"type": "string"}, "minItems": 1, "description": "event ids the judgement came from; at least one"}
            },
            "required": ["op", "kind", "name", "sources"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": {"const": "remove_node"},
              "kind": {"type": "string"},
              "name": {"type": "string"},
              "reason": {"type": "string"},
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"}
            },
            "required": ["op", "kind", "name", "reason"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": {"const": "add_edge"},
              "kind": {"type": "string"},
              "from": {
                "type": "object",
                "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                "required": ["kind", "name"],
                "additionalProperties": false
              },
              "to": {
                "type": "object",
                "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                "required": ["kind", "name"],
                "additionalProperties": false
              },
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"}
            },
            "required": ["op", "kind", "from", "to"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": {"const": "remove_edge"},
              "kind": {"type": "string"},
              "from": {
                "type": "object",
                "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                "required": ["kind", "name"],
                "additionalProperties": false
              },
              "to": {
                "type": "object",
                "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                "required": ["kind", "name"],
                "additionalProperties": false
              },
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"}
            },
            "required": ["op", "kind", "from", "to"],
            "additionalProperties": false
          }
        ]
      }
    }
  },
  "required": ["map", "changes"],
  "additionalProperties": false
}"#;

/// A node named the way a writer knows it - by kind and name - matching
/// `NodeRef`, but its own type since the domain stays serde-free.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeRefArgs {
    kind: String,
    name: String,
}

impl From<NodeRefArgs> for NodeRef {
    fn from(node: NodeRefArgs) -> Self {
        NodeRef {
            kind: node.kind,
            name: node.name,
        }
    }
}

/// One change the model asks for. `op` picks the shape, mirroring
/// `Mutation` - which this becomes once its `sources` resolve to
/// events the log actually carries.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum ChangeArgs {
    AddNode {
        kind: String,
        name: String,
        #[serde(default)]
        properties: BTreeMap<String, String>,
        #[serde(default)]
        sources: Vec<String>,
    },
    RemoveNode {
        kind: String,
        name: String,
        reason: String,
        #[serde(default)]
        sources: Vec<String>,
    },
    AddEdge {
        kind: String,
        from: NodeRefArgs,
        to: NodeRefArgs,
        #[serde(default)]
        sources: Vec<String>,
    },
    RemoveEdge {
        kind: String,
        from: NodeRefArgs,
        to: NodeRefArgs,
        #[serde(default)]
        sources: Vec<String>,
    },
}

/// Unknown keys are refused rather than ignored, the same rule every
/// other tool's arguments follow.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    map: String,
    changes: Vec<ChangeArgs>,
}

impl Tool for ReviseMap {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    /// Folds the named map, then applies each change to that one fold
    /// in order, so a later change can reference a node an earlier one
    /// just added. The first change whose `sources` name an event the
    /// log lacks, or that `Map::apply` refuses, ends the batch: its
    /// error is prefixed `change N`, N the change's 0-based index in
    /// `changes`, and nothing commits - an `Err` carries no payloads
    /// for `App` to commit. On success, `content` is one line per
    /// change, in order.
    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)
            .map_err(|e| format!("arguments do not fit revise_map's schema: {e}"))?;
        if args.changes.is_empty() {
            return Err("changes must not be empty".into());
        }
        let mut snapshot = Snapshot::load(self.log.as_ref(), &args.map, &self.scope)?;
        let mut lines = Vec::with_capacity(args.changes.len());
        let mut commits = Vec::with_capacity(args.changes.len());

        for (index, change) in args.changes.into_iter().enumerate() {
            let (line, payload) =
                apply(&mut snapshot, change).map_err(|err| format!("change {index}: {err}"))?;
            lines.push(line);
            commits.push(payload);
        }

        Ok(ToolOutput {
            content: lines.join("\n"),
            commits,
        })
    }
}

/// Turns one `ChangeArgs` into the `Mutation` `Map::apply` checks,
/// applies it, and describes what was recorded. A node's minted id
/// comes from the `Payload` `apply` returns, since nothing else knows
/// it yet.
fn apply(
    snapshot: &mut Snapshot,
    change: ChangeArgs,
) -> Result<(String, Payload), Box<dyn std::error::Error>> {
    let adding_edge = matches!(change, ChangeArgs::AddEdge { .. });
    let (mutation, line) = match change {
        ChangeArgs::AddNode {
            kind,
            name,
            properties,
            sources,
        } => {
            // The model is held to the design's rule that a cognitive
            // commit cites experience; the shell is not, so the check is
            // here and not in `Map::apply`. An edge joins two cited
            // nodes and inherits their provenance.
            if sources.is_empty() {
                return Err(format!(
                    "{kind} {name:?} cites no sources; a node needs at least one event id, as search_events returns them"
                )
                .into());
            }
            let line = format!("added {kind} {name:?}");
            let mutation = Mutation::AddNode {
                kind,
                name,
                properties,
                sources: snapshot.resolve(&sources)?,
            };
            (mutation, line)
        }
        ChangeArgs::RemoveNode {
            kind,
            name,
            reason,
            sources,
        } => {
            let node = NodeRef { kind, name };
            let line = format!("removed {node}");
            let mutation = Mutation::RemoveNode {
                node,
                reason,
                sources: snapshot.resolve(&sources)?,
            };
            (mutation, line)
        }
        ChangeArgs::AddEdge {
            kind,
            from,
            to,
            sources,
        }
        | ChangeArgs::RemoveEdge {
            kind,
            from,
            to,
            sources,
        } => {
            let (from, to): (NodeRef, NodeRef) = (from.into(), to.into());
            let sources = snapshot.resolve(&sources)?;
            let verb = if adding_edge { "added" } else { "removed" };
            let line = format!("{verb} edge {from} {kind} {to}");
            let mutation = if adding_edge {
                Mutation::AddEdge {
                    kind,
                    from,
                    to,
                    sources,
                }
            } else {
                Mutation::RemoveEdge {
                    kind,
                    from,
                    to,
                    sources,
                }
            };
            (mutation, line)
        }
    };
    let payload = snapshot.apply(mutation)?;
    let line = match &payload {
        Payload::NodeAdded { node, .. } => format!("{line} as {}", node.as_uuid()),
        _ => line,
    };
    Ok((line, payload))
}

#[cfg(test)]
mod tests;
