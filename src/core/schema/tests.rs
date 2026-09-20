use super::*;
use crate::core::testing::{debates, schemas};

#[test]
fn default_prefix_is_the_name_s_first_letter_lowercased() {
    assert_eq!(default_prefix("Verdict"), "v");
    assert_eq!(default_prefix("chore"), "c");
}

#[test]
fn kind_new_defaults_its_prefix() {
    assert_eq!(NodeKind::new("fact").prefix, "f");
}

#[test]
fn an_unknown_map_with_no_schemas_says_maps_are_none() {
    let schemas = Schemas::new(Vec::new());
    assert_eq!(
        schemas.find("decisions").err().unwrap().to_string(),
        "no map named \"decisions\"; maps are none"
    );
}

#[test]
fn a_schema_is_found_by_name() {
    let schemas = schemas();
    assert_eq!(schemas.find("debates").unwrap().name, "debates");
    assert_eq!(
        schemas.find("glossary").err().unwrap().to_string(),
        "no map named \"glossary\"; maps are debates, chores"
    );
}

#[test]
fn a_topic_declares_no_properties() {
    assert!(debates().node_kind("topic").unwrap().properties.is_empty());
}

#[test]
fn an_undeclared_kind_is_absent() {
    assert!(debates().node_kind("glossary").is_none());
}
