use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, Mutation, NodeRef, Payload};
use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::store::Snapshot;

/// The `revise_map` tool: checks a batch of changes to one map against
/// its current state, in order, and hands back the payloads to commit -
/// all of them, or none. `App` commits them, caused by the call; this
/// tool never writes the log itself.
pub struct ReviseMap {
    log: Arc<dyn EventLog>,
}

impl ReviseMap {
    pub fn new(log: Arc<dyn EventLog>) -> Self {
        Self { log }
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
        let mut snapshot = Snapshot::load(self.log.as_ref(), &args.map)?;
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
mod tests {
    use super::*;
    use crate::percept::{Actor, Event, EventId};
    use crate::testing::FakeLog;

    fn tool(events: Vec<Event>) -> ReviseMap {
        let log = Arc::new(FakeLog::seeded(events));
        ReviseMap::new(log)
    }

    #[test]
    fn spec_names_the_tool_and_carries_valid_schema_json() {
        let spec = tool(Vec::new()).spec();
        assert_eq!(spec.name, "revise_map");
        let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn a_valid_batch_returns_the_payloads_and_content() {
        let cited = Event::message_received(
            Actor::User,
            "Go or Rust?".to_string(),
            "tui".to_string(),
            None,
        );
        let cited_id = cited.id();
        let revise = tool(vec![cited]);

        let args = format!(
            r#"{{"map":"decisions","changes":[{{"op":"add_node","kind":"option","name":"Rust","properties":{{"summary":"fast"}},"sources":["{}"]}}]}}"#,
            cited_id.as_uuid()
        );

        let output = revise.run(&args).unwrap();

        assert_eq!(output.commits.len(), 1);
        let node_id = match &output.commits[0] {
            Payload::NodeAdded {
                node,
                kind,
                name,
                properties,
                sources,
                ..
            } => {
                assert_eq!(kind, "option");
                assert_eq!(name, "Rust");
                assert_eq!(properties["summary"], "fast");
                assert_eq!(sources, &vec![cited_id]);
                *node
            }
            _ => panic!("expected a NodeAdded payload"),
        };
        assert_eq!(
            output.content,
            format!("added option \"Rust\" as {}", node_id.as_uuid())
        );
    }

    #[test]
    fn a_failing_change_names_its_index_and_commits_nothing() {
        let cited =
            Event::message_received(Actor::User, "Rust".to_string(), "tui".to_string(), None);
        let id = cited.id().as_uuid().to_string();
        let revise = tool(vec![cited]);

        let err = revise
            .run(&format!(
                r#"{{"map":"decisions","changes":[
                    {{"op":"add_node","kind":"option","name":"Rust","sources":["{id}"]}},
                    {{"op":"add_node","kind":"goal","name":"Ship","sources":["{id}"]}}
                ]}}"#
            ))
            .err()
            .unwrap();

        assert!(err.to_string().starts_with("change 1: "), "{err}");
        assert_eq!(
            revise.log.load().unwrap().len(),
            1,
            "a refused batch must append nothing"
        );
    }

    #[test]
    fn a_node_with_no_sources_is_refused_and_the_error_names_the_rule() {
        let revise = tool(Vec::new());

        let err = revise
            .run(r#"{"map":"decisions","changes":[{"op":"add_node","kind":"option","name":"Rust","sources":[]}]}"#)
            .err()
            .unwrap();

        assert!(err.to_string().contains("cites no sources"), "{err}");
        assert!(
            revise.run(r#"{"map":"decisions","changes":[{"op":"add_node","kind":"option","name":"Rust"}]}"#).is_err(),
            "an omitted sources list is as empty as an empty one"
        );
    }

    #[test]
    fn an_unknown_map_is_an_error() {
        let revise = tool(Vec::new());

        let err = revise
            .run(r#"{"map":"tasks","changes":[{"op":"add_node","kind":"goal","name":"Ship","sources":[]}]}"#)
            .err()
            .unwrap();

        assert!(err.to_string().contains("decisions"), "{err}");
    }

    #[test]
    fn an_empty_changes_list_is_an_error() {
        let revise = tool(Vec::new());

        assert!(revise.run(r#"{"map":"decisions","changes":[]}"#).is_err());
    }

    #[test]
    fn a_sources_id_the_log_lacks_is_an_error() {
        let revise = tool(Vec::new());
        let unknown = EventId::new().as_uuid().to_string();

        let err = revise
            .run(&format!(
                r#"{{"map":"decisions","changes":[{{"op":"add_node","kind":"option","name":"Rust","sources":["{unknown}"]}}]}}"#
            ))
            .err()
            .unwrap();

        assert!(err.to_string().contains("no event with id"), "{err}");
    }

    #[test]
    fn a_change_can_reference_a_node_an_earlier_change_just_added() {
        let cited =
            Event::message_received(Actor::User, "Rust".to_string(), "tui".to_string(), None);
        let id = cited.id().as_uuid().to_string();
        let revise = tool(vec![cited]);

        let output = revise
            .run(&format!(
                r#"{{"map":"decisions","changes":[
                    {{"op":"add_node","kind":"question","name":"Which language?","sources":["{id}"]}},
                    {{"op":"add_node","kind":"decision","name":"Rust over Go","sources":["{id}"]}},
                    {{"op":"add_edge","kind":"resolves","from":{{"kind":"decision","name":"Rust over Go"}},"to":{{"kind":"question","name":"Which language?"}},"sources":[]}}
                ]}}"#
            ))
            .unwrap();

        assert_eq!(output.commits.len(), 3);
        assert!(matches!(output.commits[2], Payload::EdgeAdded { .. }));
        assert_eq!(
            output.content.lines().last().unwrap(),
            "added edge decision \"Rust over Go\" resolves question \"Which language?\""
        );
    }
}
