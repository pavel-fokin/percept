use super::*;
use std::sync::Arc;

use crate::app::{App, MapShape};
use crate::percept::{self, ModelDescriptor, Provider};
use crate::testing::{source, FakeCatalog, FakeLog, FakeRenderer, Scripted};
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
    }
}

fn chat_with_catalog(catalog: FakeCatalog) -> Chat<'static> {
    let app = App::new(
        Arc::new(Scripted::new(vec![], false)),
        Arc::new(catalog),
        Arc::new(FakeLog::default()),
        Vec::new(),
        Arc::new(FakeRenderer::default()),
        MapShape::Prompt,
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

fn type_str(chat: &mut Chat, text: &str) {
    for ch in text.chars() {
        chat.textarea.insert_char(ch);
    }
    chat.recompute_command_suggestions();
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
