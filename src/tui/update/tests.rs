use super::*;
use std::sync::Arc;

use crate::app::{App, MapShape};
use crate::percept::{self, ModelDescriptor, Provider};
use crate::testing::{source, FakeCatalog, FakeLog, Scripted};
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
