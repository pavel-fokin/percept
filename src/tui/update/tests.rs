use super::*;

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
