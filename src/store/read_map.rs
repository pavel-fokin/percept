use std::sync::Arc;

use serde::Deserialize;

use crate::percept::{EventLog, Tool, ToolOutput, ToolSpec};
use crate::store::fold_map;

/// The `read_map` tool: prints one map by name, the same text the
/// prompt carries when a map is sent whole. Offered when it is not, so
/// the model can open a map it judges relevant instead of reading every
/// map every turn.
pub struct ReadMap {
    log: Arc<dyn EventLog>,
}

impl ReadMap {
    pub fn new(log: Arc<dyn EventLog>) -> Self {
        Self { log }
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
        let map = fold_map(self.log.as_ref(), &args.map)?;
        if map.nodes().is_empty() {
            return Ok(ToolOutput::text(format!(
                "the {} map is empty: nothing has been recorded here yet",
                args.map
            )));
        }
        Ok(ToolOutput::text(map.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Event, EventId, Payload};
    use crate::testing::FakeLog;
    use std::collections::BTreeMap;

    fn node_added(kind: &str, name: &str) -> Event {
        Event::new(
            Actor::User,
            "tui".to_string(),
            None,
            Payload::NodeAdded {
                map: "decisions".to_string(),
                node: crate::percept::NodeId::new(),
                kind: kind.to_string(),
                name: name.to_string(),
                properties: BTreeMap::new(),
                sources: vec![EventId::new()],
            },
        )
    }

    #[test]
    fn spec_names_the_tool_and_carries_valid_schema_json() {
        let tool = ReadMap::new(Arc::new(FakeLog::default()));
        let spec = tool.spec();
        assert_eq!(spec.name, "read_map");
        let schema: serde_json::Value = serde_json::from_str(spec.parameters).unwrap();
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn a_map_reads_as_its_nodes_and_edges() {
        let log = FakeLog::seeded(vec![node_added("decision", "JSONL for the log")]);
        let out = ReadMap::new(Arc::new(log))
            .run(r#"{"map":"decisions"}"#)
            .unwrap();
        assert!(out.content.contains("decision"));
        assert!(out.content.contains("JSONL for the log"));
        assert!(out.commits.is_empty());
    }

    #[test]
    fn an_empty_map_says_so() {
        let out = ReadMap::new(Arc::new(FakeLog::default()))
            .run(r#"{"map":"decisions"}"#)
            .unwrap();
        assert!(out.content.contains("empty"));
    }

    #[test]
    fn an_unknown_map_is_an_error() {
        let tool = ReadMap::new(Arc::new(FakeLog::default()));
        let Err(err) = tool.run(r#"{"map":"plans"}"#) else {
            panic!("expected an error")
        };
        assert!(err.to_string().contains("no map named"));
    }

    #[test]
    fn a_missing_name_is_an_error() {
        let tool = ReadMap::new(Arc::new(FakeLog::default()));
        assert!(tool.run("{}").is_err());
    }
}
