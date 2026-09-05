use super::*;
use crate::percept::Provider;

fn descriptor(model: &str) -> ModelDescriptor {
    ModelDescriptor {
        provider: Provider::Ollama,
        model: model.to_string(),
    }
}

#[test]
fn a_loading_menu_has_no_descriptors_yet() {
    let menu = ModelsMenu::loading(0);
    assert_eq!(menu.descriptors(), None);
    assert_eq!(menu.selected(), None);
}

#[test]
fn populating_with_no_models_reads_as_loaded_not_loading() {
    let mut menu = ModelsMenu::loading(0);
    menu.populate(Vec::new());
    assert_eq!(menu.descriptors(), Some([].as_slice()));
    assert_eq!(menu.selected(), None);
}

#[test]
fn move_down_advances_the_selection_up_to_the_last_row() {
    let mut menu = ModelsMenu::loading(0);
    menu.populate(vec![descriptor("a"), descriptor("b")]);

    menu.move_down();
    assert_eq!(menu.selected(), Some(&descriptor("b")));

    menu.move_down();
    assert_eq!(menu.selected(), Some(&descriptor("b")));
}

#[test]
fn move_up_never_goes_past_the_first_row() {
    let mut menu = ModelsMenu::loading(0);
    menu.populate(vec![descriptor("a"), descriptor("b")]);

    menu.move_up();
    assert_eq!(menu.selected(), Some(&descriptor("a")));
}

#[test]
fn populating_a_shorter_list_clamps_a_selection_past_its_end() {
    let mut menu = ModelsMenu::loading(0);
    menu.populate(vec![descriptor("a"), descriptor("b"), descriptor("c")]);
    menu.move_down();
    menu.move_down();
    assert_eq!(menu.selected(), Some(&descriptor("c")));

    menu.populate(vec![descriptor("x")]);

    assert_eq!(menu.selected(), Some(&descriptor("x")));
}

#[test]
fn a_loading_menu_carries_the_token_it_opened_with() {
    let menu = ModelsMenu::loading(7);
    assert_eq!(menu.token(), 7);
}

fn chat() -> Chat<'static> {
    use crate::app::{App, MapShape};
    use crate::testing::{source, FakeCatalog, FakeLog, FakeRenderer, Scripted};
    use std::sync::Arc;

    let app = App::new(
        Arc::new(Scripted::new(vec![], false)),
        Arc::new(FakeCatalog::default()),
        Arc::new(FakeLog::default()),
        Vec::new(),
        Arc::new(FakeRenderer::default()),
        MapShape::Prompt,
        source("test"),
    )
    .unwrap();
    Chat::new(Box::new(app))
}

fn type_str(chat: &mut Chat, text: &str) {
    for ch in text.chars() {
        chat.textarea.insert_char(ch);
    }
    chat.recompute_command_suggestions();
}

#[test]
fn typing_a_slash_shows_every_matching_command() {
    let mut chat = chat();
    type_str(&mut chat, "/");
    assert_eq!(chat.command_suggestions.len(), commands::COMMANDS.len());
}

#[test]
fn typing_past_every_matching_prefix_hides_suggestions() {
    let mut chat = chat();
    type_str(&mut chat, "/modelsx");
    assert!(chat.command_suggestions.is_empty());
}

#[test]
fn plain_text_not_starting_with_slash_shows_no_suggestions() {
    let mut chat = chat();
    type_str(&mut chat, "hello");
    assert!(chat.command_suggestions.is_empty());
}

#[test]
fn a_narrower_prefix_still_matching_shows_the_matching_command() {
    let mut chat = chat();
    type_str(&mut chat, "/model");
    assert_eq!(chat.command_suggestions.len(), 1);
    assert_eq!(chat.command_suggestions[0].name, commands::MODELS);
}
