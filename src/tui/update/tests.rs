use super::*;
use std::sync::Arc;

use crate::app::{App, Harness, MapShape};
use crate::percept::{self, ModelDescriptor, Provider};
use crate::testing::{source, FakeCatalog, FakeLog, FakeRenderer, FakeTool, FixedPolicy, Scripted};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[test]
fn slash_models_exact_match_is_the_models_command() {
    assert!(is_models_command("/models"));
}

#[test]
fn slash_models_with_surrounding_whitespace_is_the_models_command() {
    assert!(is_models_command("  /models  "));
}

#[test]
fn plain_message_starting_with_slash_models_is_not_the_command() {
    assert!(!is_models_command("/models please"));
    assert!(!is_models_command("/modelsx"));
}

#[test]
fn ordinary_message_is_not_the_models_command() {
    assert!(!is_models_command("hello there"));
}

fn descriptor(model: &str) -> ModelDescriptor {
    ModelDescriptor {
        provider: Provider::Ollama,
        model: model.to_string(),
        reasoning_efforts: &[],
    }
}

fn chat_with_catalog(catalog: FakeCatalog) -> Chat<'static> {
    let app = App::new(
        Arc::new(Scripted::new(vec![], false)),
        Arc::new(catalog),
        Arc::new(FakeLog::default()),
        Harness::new(Vec::new(), MapShape::Prompt),
        Arc::new(FakeRenderer::default()),
        source("test"),
    )
    .unwrap();
    Chat::new(Box::new(app))
}

fn chat() -> Chat<'static> {
    chat_with_catalog(FakeCatalog::default())
}

#[test]
fn ctrl_c_quits_even_while_the_models_popup_is_open() {
    let mut chat = chat();
    chat.models_menu = Some(ModelsMenu::loading(0));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let quit = handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &tx,
    )
    .unwrap();

    assert!(quit);
}

#[test]
fn esc_closes_the_popup_rather_than_quitting_while_it_is_open() {
    let mut chat = chat();
    chat.models_menu = Some(ModelsMenu::loading(0));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let quit = handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(!quit);
    assert!(chat.models_menu.is_none());
}

#[test]
fn a_successful_model_switch_clears_a_stale_error_from_an_earlier_failed_pick() {
    let picked = descriptor("b");
    let other: Arc<dyn percept::Model> = Arc::new(Scripted::new(vec![], false));
    let catalog = FakeCatalog::new(vec![picked.clone()], vec![(picked.clone(), other)]);
    let mut chat = chat_with_catalog(catalog);
    chat.error = Some("an earlier pick failed".to_string());
    chat.models_menu = Some(ModelsMenu::loading(0));
    chat.models_menu.as_mut().unwrap().populate(vec![picked]);
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.models_menu.is_none());
    assert!(chat.error.is_none());
}

#[test]
fn a_models_listed_event_whose_token_does_not_match_the_open_menu_is_dropped() {
    let mut chat = chat();
    chat.models_menu = Some(ModelsMenu::loading(5));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_stream(
        &mut chat,
        StreamEvent::ModelsListed(1, vec![descriptor("a")]),
        &tx,
    )
    .unwrap();

    assert!(chat.models_menu.unwrap().descriptors().is_none());
}

const FIRST: commands::Command = commands::Command {
    name: "/aaa",
    description: "a",
};
const SECOND: commands::Command = commands::Command {
    name: "/aab",
    description: "b",
};

/// Two suggestions with no typed prefix filtering them out, so arrow
/// movement between them can be tested independent of `COMMANDS`
/// having only one real command today.
fn chat_with_two_suggestions() -> Chat<'static> {
    let mut chat = chat();
    chat.command_suggestions = vec![&FIRST, &SECOND];
    chat
}

#[test]
fn arrow_down_moves_the_highlighted_suggestion() {
    let mut chat = chat_with_two_suggestions();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert_eq!(chat.command_selected, 1);
}

#[test]
fn arrow_up_never_goes_past_the_first_suggestion() {
    let mut chat = chat_with_two_suggestions();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert_eq!(chat.command_selected, 0);
}

#[test]
fn arrow_down_never_goes_past_the_last_suggestion() {
    let mut chat = chat_with_two_suggestions();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();
    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert_eq!(chat.command_selected, 1);
}

#[test]
fn tab_replaces_the_line_with_the_highlighted_commands_name() {
    let mut chat = chat();
    type_str(&mut chat, "/");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert_eq!(chat.textarea.lines().join("\n"), commands::MODELS);
}

#[test]
fn tab_closes_the_dropdown_after_accepting() {
    let mut chat = chat();
    type_str(&mut chat, "/");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.command_suggestions.is_empty());
}

#[test]
fn esc_closes_the_dropdown_without_quitting() {
    let mut chat = chat();
    type_str(&mut chat, "/");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let quit = handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(!quit);
    assert!(chat.command_suggestions.is_empty());
}

#[test]
fn esc_quits_when_no_dropdown_is_open() {
    let mut chat = chat();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let quit = handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(quit);
}

#[tokio::test(flavor = "current_thread")]
async fn enter_still_submits_normally_while_the_dropdown_is_open_for_ordinary_text() {
    let mut chat = chat();
    // "/model" matches "/models" as a prefix, so the dropdown is open,
    // but it's not the exact `/models` command - Enter should submit
    // it as ordinary text rather than opening the models popup.
    type_str(&mut chat, "/model");
    assert!(!chat.command_suggestions.is_empty());
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    let quit = handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(!quit);
    assert!(chat.models_menu.is_none());
    assert!(chat.textarea.lines().join("\n").is_empty());
}

#[test]
fn ctrl_j_newline_refreshes_stale_command_suggestions() {
    let mut chat = chat_with_two_suggestions();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
        &tx,
    )
    .unwrap();

    assert!(chat.command_suggestions.is_empty());
}

/// A chat whose model may call `search_events`, and whose policy puts
/// every call to the user.
fn chat_asking() -> Chat<'static> {
    let app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Harness {
            policy: Arc::new(FixedPolicy(percept::Verdict::Ask)),
            ..Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt)
        },
        Arc::new(FakeRenderer::default()),
        source("test"),
    )
    .unwrap();
    Chat::new(Box::new(app))
}

fn tool_call() -> StreamEvent {
    StreamEvent::Chunk(Chunk::ToolCall {
        tool: "search_events".to_string(),
        arguments: "{}".to_string(),
    })
}

#[test]
fn a_call_the_policy_asks_about_waits_on_an_approval_instead_of_running() {
    let mut chat = chat_asking();
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    assert!(chat.approval.is_some());
    assert_eq!(chat.app.events().len(), 2);
}

#[test]
fn a_tool_call_past_the_budget_ends_the_turn_instead_of_hanging() {
    let app = App::new(
        Arc::new(Scripted::new(vec![], true)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Harness {
            tool_cap: 0,
            ..Harness::new(vec![Arc::new(FakeTool)], MapShape::Prompt)
        },
        Arc::new(FakeRenderer::default()),
        source("test"),
    )
    .unwrap();
    let mut chat = Chat::new(Box::new(app));
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    assert!(matches!(rx.try_recv(), Ok(StreamEvent::Ended(None))));
    assert!(!chat.app.is_replying());
}

#[tokio::test(flavor = "current_thread")]
async fn n_declines_the_waiting_call_and_the_turn_goes_on() {
    let mut chat = chat_asking();
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.approval.is_none());
    assert!(matches!(
        chat.app.events()[2].payload(),
        percept::Payload::ToolResulted { content } if content.starts_with("The user declined")
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn y_runs_the_waiting_call_and_its_result_comes_back_as_a_stream_event() {
    let mut chat = chat_asking();
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.approval.is_none());
    assert!(matches!(
        rx.recv().await,
        Some(StreamEvent::ToolResult(output)) if output.content == "ran"
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn a_runs_the_waiting_call_and_the_next_call_of_that_tool_never_asks() {
    let mut chat = chat_asking();
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.approval.is_none());
    let Some(StreamEvent::ToolResult(output)) = rx.recv().await else {
        panic!("the call ran");
    };
    let _ = chat.app.finish_tool(output).unwrap();
    handle_stream(&mut chat, tool_call(), &tx).unwrap();
    assert!(chat.approval.is_none(), "the second call ran unasked");
}

#[test]
fn typing_while_a_call_waits_reaches_neither_the_textarea_nor_the_call() {
    let mut chat = chat_asking();
    let _ = chat.app.submit("go".to_string()).unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    handle_stream(&mut chat, tool_call(), &tx).unwrap();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.approval.is_some());
    assert!(chat.current_text().is_empty());
}

#[test]
fn slash_undo_without_a_snapshot_shows_why_it_cannot() {
    let mut chat = chat();
    type_str(&mut chat, "/undo");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.current_text().is_empty());
    assert!(chat
        .error
        .as_deref()
        .unwrap()
        .starts_with("no snapshots are kept"));
    assert!(chat.app.events().is_empty());
}

#[test]
fn slash_context_puts_a_report_starting_with_the_first_section_in_the_notice() {
    let mut chat = chat();
    type_str(&mut chat, "/context");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_key(
        &mut chat,
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        &tx,
    )
    .unwrap();

    assert!(chat.current_text().is_empty());
    assert!(chat.notice.as_deref().unwrap().starts_with("instructions"));
}

#[test]
fn a_models_listed_event_with_a_matching_token_populates_the_menu() {
    let mut chat = chat();
    chat.models_menu = Some(ModelsMenu::loading(5));
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

    handle_stream(
        &mut chat,
        StreamEvent::ModelsListed(5, vec![descriptor("a")]),
        &tx,
    )
    .unwrap();

    assert!(chat.models_menu.unwrap().descriptors().is_some());
}
