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
