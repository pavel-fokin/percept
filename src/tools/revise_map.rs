use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::core::{Actor, EventLog, Map, Mutation, NodeRef, Payload, Schemas, Scope, DECISION};
use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::mapstore::{NodeRefArgs, Snapshot};

/// The `revise_map` tool: checks a batch of changes to one map against
/// its current state, in order, and hands back the payloads to commit -
/// all of them, or none. `App` commits them, caused by the call; this
/// tool never writes the log itself.
pub struct ReviseMap {
    log: Arc<dyn EventLog>,
    schemas: Arc<Schemas>,
    scope: Scope,
}

impl ReviseMap {
    pub fn new(log: Arc<dyn EventLog>, schemas: Arc<Schemas>, scope: Scope) -> Self {
        Self {
            log,
            schemas,
            scope,
        }
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
    not by an id you choose. Correct a decision by adding the new one \
    with a supersedes edge to the old, not by removing the old: a node \
    the user wrote cannot be removed by you at all.";

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
              "op": {"const": "change_node"},
              "node": {
                "description": "the node to change: {kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
              },
              "name": {"type": "string", "description": "a rename, if any"},
              "properties": {"type": "object", "additionalProperties": {"type": "string"}, "description": "merged into the node's own; a key given here replaces that key alone"},
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"},
              "why": {"type": "string", "description": "why this change is made, if there is more to say than the properties themselves; a change naming neither a rename nor a property, only this, is a comment"}
            },
            "required": ["op", "node"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": {"const": "remove_node"},
              "node": {
                "description": "the node to remove: {kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
              },
              "why": {"type": "string"},
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"}
            },
            "required": ["op", "node", "why"],
            "additionalProperties": false
          },
          {
            "type": "object",
            "properties": {
              "op": {"const": "add_edge"},
              "kind": {"type": "string"},
              "from": {
                "description": "{kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
              },
              "to": {
                "description": "{kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
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
                "description": "{kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
              },
              "to": {
                "description": "{kind, name}, or the short id its map shows it as, e.g. d41",
                "oneOf": [
                  {
                    "type": "object",
                    "properties": {"kind": {"type": "string"}, "name": {"type": "string"}},
                    "required": ["kind", "name"],
                    "additionalProperties": false
                  },
                  {"type": "string"}
                ]
              },
              "sources": {"type": "array", "items": {"type": "string"}, "description": "event ids the judgement came from"},
              "why": {"type": "string"}
            },
            "required": ["op", "kind", "from", "to", "why"],
            "additionalProperties": false
          }
        ]
      }
    }
  },
  "required": ["map", "changes"],
  "additionalProperties": false
}"#;

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
    ChangeNode {
        node: NodeRefArgs,
        name: Option<String>,
        #[serde(default)]
        properties: BTreeMap<String, String>,
        #[serde(default)]
        sources: Vec<String>,
        why: Option<String>,
    },
    RemoveNode {
        node: NodeRefArgs,
        why: String,
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
        why: String,
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
        let mut snapshot = Snapshot::load(self.log.as_ref(), &self.schemas, &args.map, &self.scope)?;
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
    let (mutation, line) = match change {
        ChangeArgs::AddNode {
            kind,
            name,
            properties,
            sources,
        } => {
            cited(&sources, format_args!("{kind} {name:?}"))?;
            let line = format!("added {kind} {name:?}");
            let mutation = Mutation::AddNode {
                kind,
                name,
                properties,
                sources: snapshot.resolve(&sources)?,
            };
            (mutation, line)
        }
        ChangeArgs::ChangeNode {
            node,
            name,
            properties,
            sources,
            why,
        } => {
            let node = node_ref(snapshot.map(), node)?;
            cited(&sources, format_args!("{node}"))?;
            let line = match &name {
                Some(new_name) => format!("changed {node} to {new_name:?}"),
                None => format!("changed {node}"),
            };
            let mutation = Mutation::ChangeNode {
                node,
                name,
                properties,
                sources: snapshot.resolve(&sources)?,
                why,
            };
            (mutation, line)
        }
        ChangeArgs::RemoveNode { node, why, sources } => {
            let node = node_ref(snapshot.map(), node)?;
            if node.kind == DECISION {
                return Err(format!(
                    "{node} is a decision, and a decision is never removed; add the one \
                     that replaces it with a supersedes edge to this one"
                )
                .into());
            }
            if let Some(guard) = user_guards_node(snapshot.map(), &node) {
                return Err(guard.into());
            }
            let line = format!("removed {node}");
            let mutation = Mutation::RemoveNode {
                node,
                why,
                sources: snapshot.resolve(&sources)?,
            };
            (mutation, line)
        }
        ChangeArgs::AddEdge {
            kind,
            from,
            to,
            sources,
        } => {
            let from = node_ref(snapshot.map(), from)?;
            let to = node_ref(snapshot.map(), to)?;
            let sources = snapshot.resolve(&sources)?;
            let line = format!("added edge {from} {kind} {to}");
            let mutation = Mutation::AddEdge {
                kind,
                from,
                to,
                sources,
            };
            (mutation, line)
        }
        ChangeArgs::RemoveEdge {
            kind,
            from,
            to,
            sources,
            why,
        } => {
            let from = node_ref(snapshot.map(), from)?;
            let to = node_ref(snapshot.map(), to)?;
            if user_wrote_edge(snapshot.map(), &kind, &from, &to) {
                return Err(format!(
                    "edge {from} {kind} {to} was written by the user and the model may not remove it"
                )
                .into());
            }
            let sources = snapshot.resolve(&sources)?;
            let line = format!("removed edge {from} {kind} {to}");
            let mutation = Mutation::RemoveEdge {
                kind,
                from,
                to,
                sources,
                why,
            };
            (mutation, line)
        }
    };
    let payload = snapshot.apply(mutation, Actor::Agent)?;
    let line = match &payload {
        Payload::NodeAdded { node, .. } => format!("{line} as {}", node.as_uuid()),
        _ => line,
    };
    Ok((line, payload))
}

/// Resolves `args` - `{kind, name}` or a short id - against `map` to
/// the `kind:name` a `Mutation` takes, the way the CLI's own `--from`,
/// `--to`, and node arguments do.
fn node_ref(map: &Map, args: NodeRefArgs) -> Result<NodeRef, Box<dyn std::error::Error>> {
    let id = args.resolve(map)?;
    let node = map.node(id).expect("resolve returns a live node's id");
    Ok(NodeRef {
        kind: node.kind.clone(),
        name: node.name.clone(),
    })
}

/// Why the model may not remove `node`, if the user's marks stand in
/// the way: the node is the user's, or a user-written edge touches it -
/// removing a node drops its edges, so that edge guards its ends too.
/// `None` when the node is free to go, or the map lacks it - `apply`
/// reports that.
fn user_guards_node(map: &Map, node: &NodeRef) -> Option<String> {
    let found = map.find(&node.kind, &node.name)?;
    if matches!(found.actor, Actor::Human(_)) {
        return Some(format!(
            "{node} was written by the user and the model may not remove it; \
             add the corrected node and a supersedes edge from it to this one instead"
        ));
    }
    map.edges()
        .iter()
        .find(|edge| matches!(edge.actor, Actor::Human(_)) && (edge.from == found.id || edge.to == found.id))
        .map(|edge| {
            format!(
                "{node} cannot be removed by the model: the user wrote the edge {}, \
                 which removing the node would drop",
                map.edge_line(edge)
            )
        })
}

/// Whether `map` holds the edge `from kind to` and the user wrote it. An
/// edge the map lacks is not the user's; `apply` reports it missing.
fn user_wrote_edge(map: &Map, kind: &str, from: &NodeRef, to: &NodeRef) -> bool {
    let Some(from) = map.find(&from.kind, &from.name) else {
        return false;
    };
    let Some(to) = map.find(&to.kind, &to.name) else {
        return false;
    };
    map.edges().iter().any(|edge| {
        edge.kind == kind && edge.from == from.id && edge.to == to.id && matches!(edge.actor, Actor::Human(_))
    })
}

/// The model is held to the design's rule that a cognitive commit
/// cites experience; the shell is not, so the check is here and not in
/// `Map::apply`. An edge joins two cited nodes and inherits their
/// provenance, so only a node's add and change go through this.
fn cited(sources: &[String], what: std::fmt::Arguments<'_>) -> Result<(), Box<dyn std::error::Error>> {
    if sources.is_empty() {
        return Err(format!(
            "{what} cites no sources; at least one event id is needed, as search_events returns them"
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
