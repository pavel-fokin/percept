use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::core::testing::debates;
use crate::core::Rules;

#[test]
fn for_schema_moment_is_none_when_the_schema_declares_no_lines_for_it() {
    let schema = debates();

    assert!(for_schema_moment(&schema, Path::new("/checkout"), "reflection.started").is_none());
}

#[test]
fn for_schema_moment_carries_only_that_schema_own_lines() {
    let schema = Schema {
        rules: Rules::new(BTreeMap::from([(
            "reflection.started".to_string(),
            vec!["revise decisions from recent events".to_string()],
        )])),
        ..debates()
    };

    let rendered = for_schema_moment(&schema, Path::new("/checkout"), "reflection.started").unwrap();

    assert!(rendered.contains("/checkout/.percept/schemas"), "{rendered}");
    assert!(rendered.contains("revise decisions from recent events"), "{rendered}");
}
