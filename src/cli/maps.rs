use std::collections::BTreeMap;
use std::path::Path;

use super::hook::Client;
use crate::app::{AppService, Ref, source_path};
use crate::core::{Actor, Definition, DefinitionStore, EventStore, Map, Reference, Source};
use crate::shared::Result;

pub(super) async fn create<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, kind: &str, file: &Path) -> Result<()> {
    let (actor, source) = cli_writer().await?;
    let project = service.project_at(source.path()?).await?.id;
    let file = tokio::fs::canonicalize(file).await.map_err(|e| format!("{}: {e}", file.display()))?;

    service.create_map(source, actor, project, kind, &file).await
}

pub(super) async fn add<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, args: &[String]) -> Result<()> {
    let AddArgs { positionals, properties, references } = split_add(args)?;

    match positionals {
        [map, kind, title] => {
            let (actor, source) = cli_writer().await?;
            let project = service.project_at(source.path()?).await?.id;
            service.add_node(source, actor, project, map, kind, title, properties, references).await
        }
        [map, from, kind, to] => {
            let (actor, source) = cli_writer().await?;
            let project = service.project_at(source.path()?).await?.id;
            service.add_edge(source, actor, project, map, from, kind, to).await
        }
        _ => Err("usage: percept add <map> <kind> <title> [--key value ...] [--ref 'msg:<text>' | --ref 'file:<path>:<from>-<to>' ...] | percept add <map> <from> <kind> <to>".into()),
    }
}

pub(super) async fn change<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, args: &[String]) -> Result<()> {
    let AddArgs { positionals, properties, references } = split_add(args)?;

    let (map, title, new_title) = match positionals {
        [map, title] => (map, title, None),
        [map, title, new_title] => (map, title, Some(new_title.as_str())),
        _ => return Err("usage: percept change <map> <title> [<new title>] [--key value ...] [--ref 'msg:<text>' | --ref 'file:<path>:<from>-<to>' ...]".into()),
    };
    let (actor, source) = cli_writer().await?;
    let project = service.project_at(source.path()?).await?.id;

    service.change_node(source, actor, project, map, title, new_title, properties, references).await
}

pub(super) async fn remove<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, args: &[String]) -> Result<()> {
    match split_remove(args)? {
        [map, title] => {
            let (actor, source) = cli_writer().await?;
            let project = service.project_at(source.path()?).await?.id;
            service.remove_node(source, actor, project, map, title).await
        }
        [map, from, kind, to] => {
            let (actor, source) = cli_writer().await?;
            let project = service.project_at(source.path()?).await?.id;
            service.remove_edge(source, actor, project, map, from, kind, to).await
        }
        _ => Err("usage: percept remove <map> <title> | percept remove <map> <from> <kind> <to>".into()),
    }
}

fn split_remove(args: &[String]) -> Result<&[String]> {
    if args.iter().any(|a| a.starts_with("--")) {
        return Err("remove takes no properties or references".into());
    }

    Ok(args)
}

struct AddArgs<'a> {
    positionals: &'a [String],
    properties: BTreeMap<String, String>,
    references: Vec<Ref>,
}

fn split_add(args: &[String]) -> Result<AddArgs<'_>> {
    let (positionals, pairs) = args.split_at(args.iter().position(|a| a.starts_with("--")).unwrap_or(args.len()));
    let mut properties = BTreeMap::new();
    let mut references = Vec::new();

    for pair in pairs.chunks(2) {
        let key = pair[0].strip_prefix("--").filter(|key| !key.is_empty()).ok_or("a property key is empty or lacks its --")?;
        let value = pair.get(1).ok_or_else(|| format!("--{key} has no value"))?;

        if key == "ref" {
            references.push(parse_reference(value)?);
        } else if properties.insert(key.to_string(), value.clone()).is_some() {
            return Err(format!("--{key} is given more than once").into());
        }
    }

    if positionals.len() == 4 {
        if !properties.is_empty() {
            return Err("an edge takes no properties".into());
        }
        if !references.is_empty() {
            return Err("an edge takes no references".into());
        }
    }

    Ok(AddArgs { positionals, properties, references })
}

fn parse_reference(value: &str) -> Result<Ref> {
    if let Some(quote) = value.strip_prefix("msg:") {
        if quote.trim().is_empty() {
            return Err("a message reference must not be blank".into());
        }
        return Ok(Ref::Message(quote.to_string()));
    }
    if let Some(file) = value.strip_prefix("file:") {
        let (path, range) = file.rsplit_once(':').ok_or("a file reference expects file:<path>:<from>-<to>")?;
        let (from, to) = range.split_once('-').ok_or("a file reference expects file:<path>:<from>-<to>")?;
        let from = from.parse::<u32>().map_err(|_| "a file reference line must be a positive integer")?;
        let to = to.parse::<u32>().map_err(|_| "a file reference line must be a positive integer")?;

        if path.is_empty() || std::path::Path::new(path).is_absolute() {
            return Err("a file reference path must be nonempty and relative to the project".into());
        }
        if from == 0 || from > to {
            return Err("a file reference range must use 1-based lines with from <= to".into());
        }

        return Ok(Ref::File { path: path.into(), lines: (from, to) });
    }

    Err("--ref expects msg:<text> or file:<path>:<from>-<to>".into())
}

pub(super) async fn cli_writer() -> Result<(Actor, Source)> {
    let client = writer(env_flag("CLAUDECODE"), env_flag("CODEX_SESSION_ID"))?;
    let actor = if client.is_some() { Actor::Agent } else { Actor::Human };
    let name = client.map_or("percept-cli", Client::name);
    let source = Source::Checkout { client: name.into(), path: source_path(&std::env::current_dir()?).await };

    Ok((actor, source))
}

pub(super) async fn show<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, map: Option<&str>) -> Result<()> {
    let project = service.project_at(&source_path(&std::env::current_dir()?).await).await?.id;

    match map {
        Some(kind) => println!("{}", map_markdown(&service.map(project, kind).await?)?),
        None => {
            for map in service.maps(project).await? {
                println!("{}  {}, {}", map.kind, node_count(map.nodes.len()), edge_count(map.edges.len()));
            }
        }
    }

    Ok(())
}

pub(super) async fn describe<S: EventStore, M: DefinitionStore>(service: &AppService<S, M>, kind: &str) -> Result<()> {
    let project = service.project_at(&source_path(&std::env::current_dir()?).await).await?.id;
    let definition = service.definition(project, kind).await?;

    println!("{}", definition_markdown(kind, &definition));

    Ok(())
}

fn definition_markdown(kind: &str, definition: &Definition) -> String {
    let mut text = format!(
        "# {kind}\n\n{}\n\n## Instructions\n\n{}\n\n## Schema\n\n| Node kind | Properties |\n|---|---|\n",
        definition.purpose().trim(),
        definition.instructions().trim(),
    );

    for (node, properties) in definition.nodes() {
        let properties: Vec<&str> = properties.iter().map(String::as_str).collect();
        text.push_str(&format!("| {node} | {} |\n", properties.join(", ")));
    }

    if !definition.edges().is_empty() {
        text.push_str("\n| Edge kind | From | To |\n|---|---|---|\n");
    }
    for (edge, ends) in definition.edges() {
        text.push_str(&format!("| {edge} | {} | {} |\n", ends.from, ends.to));
    }

    text.trim_end().to_string()
}

fn map_markdown(map: &Map) -> Result<String> {
    let mut text = format!("# {}\n", map.kind);

    if !map.nodes.is_empty() {
        text.push_str("\n## Nodes\n");
    }
    for node in &map.nodes {
        text.push_str(&format!("\n### {}: {}\n", node.kind, node.title));
        if !node.properties.is_empty() || !node.references.is_empty() {
            text.push('\n');
        }
        for (key, value) in &node.properties {
            text.push_str(&format!("- {key}: {}\n", value.replace('\n', "\n  ")));
        }
        for reference in &node.references {
            match reference {
                Reference::Message { quote, .. } => text.push_str(&format!("- ref msg: {quote:?}\n")),
                Reference::File { path, lines: (from, to), excerpt } => {
                    let fence = fence(excerpt);
                    text.push_str(&format!("- ref file: {}:{from}-{to}\n\n  {fence}\n", path.display()));
                    for line in excerpt.split('\n') {
                        if !line.is_empty() {
                            text.push_str("  ");
                        }
                        text.push_str(line);
                        text.push('\n');
                    }
                    text.push_str(&format!("  {fence}\n"));
                }
            }
        }
    }

    let title = |id| map.node(id).map(|n| n.title.as_str()).ok_or("an edge names a node that is not in its map");
    if !map.edges.is_empty() {
        text.push_str("\n## Edges\n\n");
    }
    for edge in &map.edges {
        text.push_str(&format!("- {} → {} → {}\n", title(edge.from)?, edge.kind, title(edge.to)?));
    }

    Ok(text.trim_end().to_string())
}

/// A code fence longer than any run of backticks in `text`, so `text` cannot close it.
fn fence(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);

    "`".repeat(longest.max(2) + 1)
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

/// The client running the CLI, from the values of `CLAUDECODE` and `CODEX_SESSION_ID`; none means a human.
fn writer(claudecode: Option<String>, codex_session_id: Option<String>) -> Result<Option<Client>> {
    match (claudecode.as_deref() == Some("1"), codex_session_id.is_some()) {
        (true, true) => Err("both CLAUDECODE and CODEX_SESSION_ID are set".into()),
        (true, false) => Ok(Some(Client::Claude)),
        (false, true) => Ok(Some(Client::Codex)),
        (false, false) => Ok(None),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{EdgeKind, Event, EventId, MapId, NodeId};

    #[test]
    fn writer_follows_the_client_environment() {
        let set = || Some("1".to_string());

        assert_eq!(writer(set(), None).unwrap(), Some(Client::Claude));
        assert_eq!(writer(None, set()).unwrap(), Some(Client::Codex));
        assert_eq!(writer(None, None).unwrap(), None);
        assert!(writer(set(), set()).is_err());
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn properties_are_key_value_pairs() {
        let args = strings(&["m", "k", "T", "--definition", "One log.", "--note", "-x"]);
        let found = split_add(&args).unwrap().properties;

        assert_eq!(found.len(), 2);
        assert_eq!(found["definition"], "One log.");
        assert_eq!(found["note"], "-x");
        assert!(split_add(&[]).unwrap().properties.is_empty());
    }

    #[test]
    fn a_property_without_a_value_is_an_error() {
        assert!(split_add(&strings(&["--definition"])).is_err());
        assert!(split_add(&strings(&["--a", "1", "--b"])).is_err());
    }

    #[test]
    fn a_repeated_property_key_is_an_error() {
        assert!(split_add(&strings(&["--a", "1", "--a", "2"])).is_err());
    }

    #[test]
    fn an_empty_property_key_is_an_error() {
        assert!(split_add(&strings(&["--", "1"])).is_err());
    }

    #[test]
    fn properties_on_an_edge_are_an_error() {
        let edge_args = strings(&["m", "a", "k", "b", "--definition", "x"]);
        let node_args = strings(&["m", "k", "T", "--definition", "x"]);

        let edge = split_add(&edge_args);
        let node = split_add(&node_args).unwrap();

        assert!(edge.is_err());
        assert_eq!(node.positionals.len(), 3);
        assert_eq!(node.properties["definition"], "x");
    }

    #[test]
    fn references_preserve_order_colons_dashes_and_whitespace() {
        let args = strings(&[
            "m", "k", "T", "--ref", "msg: --flag: value ", "--definition", "-x",
            "--ref", "msg:another: quote", "--ref", "msg: --flag: value ",
        ]);

        let found = split_add(&args).unwrap();

        assert_eq!(found.positionals, &args[..3]);
        assert_eq!(found.properties, BTreeMap::from([("definition".into(), "-x".into())]));
        assert_eq!(found.references, vec![
            Ref::Message(" --flag: value ".into()),
            Ref::Message("another: quote".into()),
            Ref::Message(" --flag: value ".into()),
        ]);
    }

    #[test]
    fn references_require_a_known_prefix_and_nonblank_quote() {
        for value in ["", "msg:", "msg: ", "message:text", "unknown:a.rs:1-2", "text"] {
            let args = strings(&["m", "k", "T", "--ref", value]);

            assert!(split_add(&args).is_err(), "accepted {value:?}");
        }
        assert!(split_add(&strings(&["m", "k", "T", "--ref"])).is_err());
    }

    #[test]
    fn file_references_split_the_range_at_the_last_colon() {
        let args = strings(&[
            "m", "k", "T", "--ref", "file:src/a:b.rs:2-4", "--ref", "msg:words",
            "--ref", "file:./src/../src/a:b.rs:1-1",
        ]);

        let found = split_add(&args).unwrap();

        assert_eq!(found.references, vec![
            Ref::File { path: "src/a:b.rs".into(), lines: (2, 4) },
            Ref::Message("words".into()),
            Ref::File { path: "./src/../src/a:b.rs".into(), lines: (1, 1) },
        ]);
    }

    #[test]
    fn file_references_require_a_relative_path_and_explicit_valid_range() {
        for value in [
            "file:", "file::1-2", "file:/a.rs:1-2", "file:a.rs", "file:a.rs:1",
            "file:a.rs:0-1", "file:a.rs:2-1", "file:a.rs:1-", "file:a.rs:-1-2",
            "file:a.rs:a-b", "file:a.rs:1-4294967296", "file:a.rs:1-2-3",
        ] {
            let args = strings(&["m", "k", "T", "--ref", value]);

            assert!(split_add(&args).is_err(), "accepted {value:?}");
        }
    }

    #[test]
    fn references_on_an_edge_are_an_error() {
        for value in ["msg:quote", "file:a.rs:1-2"] {
            let args = strings(&["m", "a", "k", "b", "--ref", value]);

            assert!(split_add(&args).is_err());
        }
    }

    #[test]
    fn remove_takes_positionals_and_no_flags() {
        let edge = strings(&["m", "a", "k", "b"]);
        let node = strings(&["m", "a"]);

        assert_eq!(split_remove(&edge).unwrap(), &edge[..]);
        assert_eq!(split_remove(&node).unwrap(), &node[..]);
        for flag in ["--ref", "--definition", "--"] {
            let args = strings(&["m", "a", "k", "b", flag, "x"]);

            assert!(split_remove(&args).is_err(), "accepted {flag:?}");
        }
    }

    #[test]
    fn a_definition_renders_as_markdown() {
        let nodes = BTreeMap::from([
            ("decision".to_string(), ["why", "by"].map(String::from).into()),
            ("note".to_string(), Default::default()),
        ]);
        let edges = BTreeMap::from([("supersedes".to_string(), EdgeKind { from: "decision".into(), to: "decision".into() })]);
        let definition = Definition::new("Why.".into(), "\nKeep it.\n\nShort.\n".into(), nodes, edges).unwrap();

        let text = definition_markdown("decisions", &definition);

        assert_eq!(text, "# decisions\n\nWhy.\n\n## Instructions\n\nKeep it.\n\nShort.\n\n## Schema\n\n\
            | Node kind | Properties |\n|---|---|\n| decision | by, why |\n| note |  |\n\n\
            | Edge kind | From | To |\n|---|---|---|\n| supersedes | decision | decision |");
    }

    #[test]
    fn without_edge_kinds_the_edge_table_is_left_out() {
        let nodes = BTreeMap::from([("note".to_string(), Default::default())]);
        let definition = Definition::new("p".into(), "i".into(), nodes, BTreeMap::new()).unwrap();

        let text = definition_markdown("notes", &definition);

        assert!(text.ends_with("| note |  |"));
        assert!(!text.contains("Edge kind"));
    }

    #[test]
    fn a_map_renders_as_markdown() {
        let (map, question, option) = (MapId::new(), NodeId::new(), NodeId::new());
        let why = BTreeMap::from([("why".to_string(), "One log.\nNo copies.".to_string())]);
        let file = Reference::File { path: "src/a.rs".into(), lines: (1, 3), excerpt: "fn a() {\n\n}".into() };
        let quote = Reference::Message { event: EventId::new(), quote: "Keep it".into() };
        let events = [
            Event::node_added(Source::fixture(), Actor::Human, map, question, "question", "Q", BTreeMap::new(), Vec::new()),
            Event::node_added(Source::fixture(), Actor::Human, map, option, "option", "O", why, vec![file, quote]),
            Event::edge_added(Source::fixture(), Actor::Human, map, "answers", option, question),
        ];

        let text = map_markdown(&Map::fixture(map, &events)).unwrap();

        assert_eq!(text, "# concepts\n\n## Nodes\n\n### question: Q\n\n### option: O\n\n\
            - why: One log.\n  No copies.\n- ref file: src/a.rs:1-3\n\n  ```\n  fn a() {\n\n  }\n  ```\n- ref msg: \"Keep it\"\n\n\
            ## Edges\n\n- O → answers → Q");
    }

    #[test]
    fn a_fence_outgrows_the_backticks_it_holds() {
        assert_eq!(fence("plain"), "```");
        assert_eq!(fence("```rust\n```"), "````");
    }

    #[test]
    fn an_empty_map_renders_as_its_title() {
        let map = MapId::new();

        let text = map_markdown(&Map::fixture(map, &[])).unwrap();

        assert_eq!(text, "# concepts");
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
