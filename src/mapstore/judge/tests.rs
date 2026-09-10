use super::*;
use crate::core::testing::{human, node_added_by, schemas, source, FakeLog};

#[test]
fn a_blank_why_is_refused() {
    let node = node_added_by(Actor::Agent, "decisions", "Rust");
    let log = FakeLog::seeded(vec![node]);
    let schemas = schemas();
    let source = source("test");

    let result = dispute(
        &log,
        &schemas,
        &source,
        human(),
        "decisions",
        "d1",
        "   ".to_string(),
    );

    assert!(matches!(result, Err(JudgeError::BlankWhy)));
}
