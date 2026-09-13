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

#[test]
fn turn_dir_replaces_every_slash() {
    let sessions = Path::new("/data/hook-sessions");
    let root = Path::new("/Users/me/project");

    assert_eq!(
        turn_dir(sessions, root),
        sessions.join("%Users%me%project")
    );
}

#[test]
fn latest_cause_is_none_with_no_turn_open() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(TurnState::latest_cause(&dir.path().join("missing")).unwrap(), None);
}

#[test]
fn point_then_latest_cause_round_trips_creating_the_directory() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("checkout");
    let id = EventId::new();

    TurnState::point(&dir, id).unwrap();

    assert_eq!(TurnState::latest_cause(&dir).unwrap(), Some(id));
}

#[test]
fn a_later_point_replaces_the_earlier_one() {
    let dir = tempfile::tempdir().unwrap();
    let (first, second) = (EventId::new(), EventId::new());
    TurnState::point(dir.path(), first).unwrap();

    TurnState::point(dir.path(), second).unwrap();

    assert_eq!(TurnState::latest_cause(dir.path()).unwrap(), Some(second));
}

#[test]
fn unpoint_closes_the_turn_and_tolerates_none_open() {
    let dir = tempfile::tempdir().unwrap();
    TurnState::point(dir.path(), EventId::new()).unwrap();

    TurnState::unpoint(dir.path()).unwrap();
    TurnState::unpoint(dir.path()).unwrap();

    assert_eq!(TurnState::latest_cause(dir.path()).unwrap(), None);
}

#[test]
fn a_turn_file_is_not_the_pointer() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = TurnState::open(dir.path(), "codex-session").unwrap();
    state.set(EventId::new()).unwrap();

    assert_eq!(TurnState::latest_cause(dir.path()).unwrap(), None);
}
