use std::sync::Arc;

use super::*;
use crate::app::{App, Harness, MapShape};
use crate::core::testing::{content, human, schemas, source, FakeLog};
use crate::core::{EventLog, Payload};
use crate::harness::testing::{FakeCatalog, FakeTool, Scripted};

#[tokio::test(flavor = "current_thread")]
async fn ask_runs_one_tool_round_and_commits_the_final_reply() {
    let model = Scripted::new(
        vec![
            vec![crate::harness::Chunk::ToolCall {
                tool: "search_events".to_string(),
                arguments: "{}".to_string(),
            }],
            vec![crate::harness::Chunk::Reply("found it".to_string())],
        ],
        true,
    );
    let log = Arc::new(FakeLog::default());
    let tools: Vec<Arc<dyn crate::harness::Tool>> = vec![Arc::new(FakeTool)];
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(tools, MapShape::Prompt),
        source("cli"),
        human(),
    )
    .unwrap();

    run_turn(
        Box::new(app),
        Actor::Human(human()),
        "what happened".to_string(),
        false,
    )
    .await
    .unwrap();

    let events = log.load().unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].source().name, "cli");
    assert!(matches!(
        events[1].payload(),
        Payload::ToolCalled { tool, .. } if tool == "search_events"
    ));
    assert!(matches!(
        events[2].payload(),
        Payload::ToolResulted { content } if content == "ran"
    ));
    assert_eq!(content(&events[3]), "found it");
}

#[tokio::test(flavor = "current_thread")]
async fn a_stream_error_ends_the_turn_but_still_commits_partial_text() {
    let log = Arc::new(FakeLog::default());
    // A reply that breaks mid-stream, after saying something.
    let model = Scripted::failing(
        vec![vec![
            Ok(crate::harness::Chunk::Reply("partial".to_string())),
            Err("connection dropped".into()),
        ]],
        false,
    );
    let app = App::new(
        Arc::new(model),
        Arc::new(FakeCatalog::default()),
        log.clone(),
        Arc::new(schemas()),
        Harness::new(Vec::new(), MapShape::Prompt),
        source("cli"),
        human(),
    )
    .unwrap();

    let result = run_turn(Box::new(app), Actor::Human(human()), "hi".to_string(), false).await;

    assert!(result.is_err());
    let events = log.load().unwrap();
    assert_eq!(events.len(), 2);
    assert_eq!(content(&events[1]), "partial");
}
