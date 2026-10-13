use std::collections::BTreeMap;

use crate::app::{AppService, source_path};
use crate::core::{Actor, EventStore, Source};
use crate::shared::Result;

pub(super) async fn create<S: EventStore>(service: &AppService<S>, kind: &str) -> Result<()> {
    let (actor, source) = cli_writer().await?;

    service.create_map(source, actor, kind).await
}

pub(super) async fn add<S: EventStore>(service: &AppService<S>, args: &[String]) -> Result<()> {
    let (positionals, properties) = split_add(args)?;

    match positionals {
        [map, kind, title] => {
            let (actor, source) = cli_writer().await?;
            service.add_node(source, actor, map, kind, title, properties).await
        }
        [map, from, kind, to] => {
            let (actor, source) = cli_writer().await?;
            service.add_edge(source, actor, map, from, kind, to).await
        }
        _ => Err("usage: percept add <map> <kind> <title> [--key value ...] | percept add <map> <from> <kind> <to>".into()),
    }
}

/// Splits `add` arguments into positionals and properties. Only a node takes properties.
fn split_add(args: &[String]) -> Result<(&[String], BTreeMap<String, String>)> {
    let (positionals, pairs) = args.split_at(args.iter().position(|a| a.starts_with("--")).unwrap_or(args.len()));
    let properties = properties(pairs)?;

    if positionals.len() == 4 && !properties.is_empty() {
        return Err("an edge takes no properties".into());
    }

    Ok((positionals, properties))
}

/// Reads `--key value` pairs. A value may start with `-`.
fn properties(pairs: &[String]) -> Result<BTreeMap<String, String>> {
    let mut properties = BTreeMap::new();

    for pair in pairs.chunks(2) {
        let key = pair[0].strip_prefix("--").filter(|key| !key.is_empty()).ok_or("a property key is empty or lacks its --")?;
        let value = pair.get(1).ok_or_else(|| format!("--{key} has no value"))?;

        if properties.insert(key.to_string(), value.clone()).is_some() {
            return Err(format!("--{key} is given more than once").into());
        }
    }

    Ok(properties)
}

async fn cli_writer() -> Result<(Actor, Source)> {
    let (actor, client) = writer(env_flag("CLAUDECODE"), env_flag("CODEX_SESSION_ID"))?;
    let source = Source { client: client.into(), path: source_path(&std::env::current_dir()?).await };

    Ok((actor, source))
}

pub(super) async fn show<S: EventStore>(service: &AppService<S>, map: Option<&str>) -> Result<()> {
    let project = source_path(&std::env::current_dir()?).await;

    match map {
        Some(kind) => {
            let map = service.map(&project, kind).await?;
            let nodes = service.nodes(map.id).await?;
            let edges = service.edges(map.id).await?;

            println!("{}", map.kind);

            if !nodes.is_empty() {
                println!();
            }
            for node in &nodes {
                println!("{}  {}", node.kind, node.title);
                for (key, value) in &node.properties {
                    println!("  {key}: {value}");
                }
            }

            let titles: BTreeMap<_, _> = nodes.iter().map(|n| (n.id, n.title.as_str())).collect();
            let title = |id| titles.get(&id).copied().ok_or("an edge names a node that is not in its map");
            if !edges.is_empty() {
                println!();
            }
            for edge in edges {
                println!("\"{}\"  {}  \"{}\"", title(edge.from)?, edge.kind, title(edge.to)?);
            }
        }
        None => {
            for s in service.map_summaries(&project).await? {
                println!("{}  {}, {}", s.map.kind, node_count(s.nodes), edge_count(s.edges));
            }
        }
    }

    Ok(())
}

fn node_count(count: usize) -> String {
    match count {
        1 => "1 node".to_string(),
        n => format!("{n} nodes"),
    }
}

fn edge_count(count: usize) -> String {
    match count {
        1 => "1 edge".to_string(),
        n => format!("{n} edges"),
    }
}

fn env_flag(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Who is running the CLI, from the values of `CLAUDECODE` and `CODEX_SESSION_ID`.
fn writer(claudecode: Option<String>, codex_session_id: Option<String>) -> Result<(Actor, &'static str)> {
    match (claudecode.as_deref() == Some("1"), codex_session_id.is_some()) {
        (true, true) => Err("both CLAUDECODE and CODEX_SESSION_ID are set".into()),
        (true, false) => Ok((Actor::Agent, "claude")),
        (false, true) => Ok((Actor::Agent, "codex")),
        (false, false) => Ok((Actor::Human, "percept-cli")),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_follows_the_client_environment() {
        let set = || Some("1".to_string());

        assert_eq!(writer(set(), None).unwrap(), (Actor::Agent, "claude"));
        assert_eq!(writer(None, set()).unwrap(), (Actor::Agent, "codex"));
        assert_eq!(writer(None, None).unwrap(), (Actor::Human, "percept-cli"));
        assert!(writer(set(), set()).is_err());
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn properties_are_key_value_pairs() {
        let found = properties(&strings(&["--definition", "One log.", "--note", "-x"])).unwrap();

        assert_eq!(found.len(), 2);
        assert_eq!(found["definition"], "One log.");
        assert_eq!(found["note"], "-x");
        assert!(properties(&[]).unwrap().is_empty());
    }

    #[test]
    fn a_property_without_a_value_is_an_error() {
        assert!(properties(&strings(&["--definition"])).is_err());
        assert!(properties(&strings(&["--a", "1", "--b"])).is_err());
    }

    #[test]
    fn a_repeated_property_key_is_an_error() {
        assert!(properties(&strings(&["--a", "1", "--a", "2"])).is_err());
    }

    #[test]
    fn an_empty_property_key_is_an_error() {
        assert!(properties(&strings(&["--", "1"])).is_err());
    }

    #[test]
    fn properties_on_an_edge_are_an_error() {
        let edge_args = strings(&["m", "a", "k", "b", "--definition", "x"]);
        let node_args = strings(&["m", "k", "T", "--definition", "x"]);

        let edge = split_add(&edge_args);
        let node = split_add(&node_args).unwrap();

        assert!(edge.is_err());
        assert_eq!(node.0.len(), 3);
        assert_eq!(node.1["definition"], "x");
    }

    #[test]
    fn node_count_phrasings() {
        assert_eq!(node_count(0), "0 nodes");
        assert_eq!(node_count(1), "1 node");
        assert_eq!(node_count(2), "2 nodes");
    }

    #[test]
    fn edge_count_phrasings() {
        assert_eq!(edge_count(0), "0 edges");
        assert_eq!(edge_count(1), "1 edge");
        assert_eq!(edge_count(2), "2 edges");
    }
}
