use super::*;

#[test]
fn a_fresh_file_holds_no_cause() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TurnState::open(dir.path(), "turn").unwrap();

    assert_eq!(state.cause().unwrap(), None);
}

#[test]
fn set_then_cause_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TurnState::open(dir.path(), "turn").unwrap();
    let id = EventId::new();

    state.set(id).unwrap();

    assert_eq!(state.cause().unwrap(), Some(id));
}

#[test]
fn clear_empties_the_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TurnState::open(dir.path(), "turn").unwrap();
    state.set(EventId::new()).unwrap();

    state.clear().unwrap();

    assert_eq!(state.cause().unwrap(), None);
}

#[test]
fn remove_deletes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("turn");
    let state = TurnState::open(dir.path(), "turn").unwrap();

    state.remove().unwrap();

    assert!(!path.exists());
}
