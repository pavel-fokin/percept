use super::*;

fn descriptor(model: &str) -> ModelDescriptor {
    ModelDescriptor {
        provider: "ollama".to_string(),
        model: model.to_string(),
    }
}

#[test]
fn a_loading_menu_has_no_descriptors_yet() {
    let menu = ModelsMenu::loading();
    assert_eq!(menu.descriptors(), None);
    assert_eq!(menu.selected(), None);
}

#[test]
fn populating_with_no_models_reads_as_loaded_not_loading() {
    let mut menu = ModelsMenu::loading();
    menu.populate(Vec::new());
    assert_eq!(menu.descriptors(), Some([].as_slice()));
    assert_eq!(menu.selected(), None);
}

#[test]
fn move_down_advances_the_selection_up_to_the_last_row() {
    let mut menu = ModelsMenu::loading();
    menu.populate(vec![descriptor("a"), descriptor("b")]);

    menu.move_down();
    assert_eq!(menu.selected(), Some(&descriptor("b")));

    menu.move_down();
    assert_eq!(menu.selected(), Some(&descriptor("b")));
}

#[test]
fn move_up_never_goes_past_the_first_row() {
    let mut menu = ModelsMenu::loading();
    menu.populate(vec![descriptor("a"), descriptor("b")]);

    menu.move_up();
    assert_eq!(menu.selected(), Some(&descriptor("a")));
}
