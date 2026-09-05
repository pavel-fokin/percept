use super::*;

#[test]
fn every_command_name_starts_with_a_slash() {
    assert!(COMMANDS.iter().all(|c| c.name.starts_with('/')));
}

#[test]
fn models_command_is_registered() {
    assert!(COMMANDS.iter().any(|c| c.name == MODELS));
}
