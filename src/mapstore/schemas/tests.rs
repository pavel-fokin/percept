use super::*;
use crate::core::testing::Fixture;

#[test]
fn a_project_with_no_schemas_directory_has_no_maps() {
    let fixture = Fixture::new();
    let schemas = load(fixture.path()).unwrap();
    let names: Vec<&str> = schemas.folded().map(|s| s.name()).collect();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn each_shipped_template_parses_under_its_own_name() {
    for (name, text) in templates() {
        let schema = parse(&name, text).unwrap();
        assert_eq!(schema.name(), name);
    }
}

#[test]
fn the_decisions_template_declares_concept_first() {
    let (name, text) = templates().into_iter().find(|(name, _)| name == "concepts").unwrap();
    let schema = parse(&name, text).unwrap();

    // Declaration order is nesting order: a question files under a
    // concept, never the reverse.
    let kinds: Vec<&str> = schema.node_kind_names().collect();
    assert_eq!(kinds, ["concept", "question", "option", "decision"]);
}

#[test]
fn the_decisions_template_lets_a_concept_cover_a_concept() {
    let (name, text) = templates().into_iter().find(|(name, _)| name == "concepts").unwrap();
    let schema = parse(&name, text).unwrap();
    let covers = schema.edge_kind("covers").unwrap();

    assert_eq!(covers.from(), ["concept"]);
    assert_eq!(covers.to(), ["concept"]);
}

#[test]
fn a_project_schema_of_a_new_name_is_added() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"terms and their meaning\"\n\n\
         [nodes.term]\n\
         meaning = \"\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name()).collect();
    assert_eq!(names, ["glossary"]);
    let glossary = schemas.find("glossary").unwrap();
    assert!(glossary.node_kind("term").unwrap().property("meaning").is_some());
}

#[test]
fn a_toml_syntax_error_names_the_file() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/broken.toml", "purpose = \"unterminated");

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("broken.toml:"), "{err}");
    assert!(err.contains("line"), "{err}");
}

#[test]
fn a_domain_schema_error_is_prefixed_with_the_file_name() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/glossary.toml", "purpose = \"p\"\n");

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(err, "glossary.toml: declares no node kinds");
}

#[test]
fn a_settles_key_is_an_unknown_field() {
    // The core keeps no settlement pair - "settles" is not a shape any
    // schema file declares.
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\
         settles = { by = \"resolution\", of = \"term\" }\n\n\
         [nodes.term]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("settles"), "{err}");
}

#[test]
fn a_directory_named_dot_toml_is_ignored() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path().join(".percept/schemas/x.toml")).unwrap();

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name()).collect();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn a_file_with_an_unknown_key_is_refused_naming_the_key() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\nheadline = [\"term\"]\n\n[nodes.term]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("headline"), "{err}");
}

#[test]
fn a_node_kind_with_no_prefix_defaults_to_its_first_letter() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().prefix(), "t");
}

#[test]
fn a_node_kind_with_an_explicit_prefix_keeps_it() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\nprefix = \"tm\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().prefix(), "tm");
}

#[test]
fn a_prefix_declared_as_a_list_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\nprefix = [\"t\", \"tm\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("\"prefix\""), "{err}");
    assert!(err.contains("must be a string"), "{err}");
}

#[test]
fn a_closed_list_property_loads_onto_the_node_kind() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\nstate = [\"open\", \"done\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(
        glossary.node_kind("term").unwrap().closed_list(),
        Some(("state", ["open".to_string(), "done".to_string()].as_slice()))
    );
}

#[test]
fn an_edge_end_naming_a_list_loads_both_kinds() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\n\n[nodes.acronym]\n\n\
         [edges.relates]\nfrom = \"term\"\nto = [\"term\", \"acronym\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    let relates = glossary.edge_kind("relates").unwrap();
    assert_eq!(relates.to(), ["term".to_string(), "acronym".to_string()]);
}

#[test]
fn an_edge_kind_with_an_unknown_key_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\n\n\
         [edges.relates]\nfrom = \"term\"\nto = \"term\"\nvia = \"link\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("via"), "{err}");
}

#[test]
fn free_text_properties_load_onto_the_node_kind() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\nnote = \"\"\nsummary = \"\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    let term = glossary.node_kind("term").unwrap();
    assert!(term.property("note").is_some());
    assert!(term.property("summary").is_some());
}

#[test]
fn a_free_property_written_with_text_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\nnote = \"what this explains\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("note"), "{err}");
    assert!(err.contains("never read"), "{err}");
}

#[test]
fn a_rules_table_left_from_an_older_schema_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "purpose = \"p\"\n\n[nodes.term]\n\n[rules]\n\"message.received\" = [\"a\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("glossary.toml"), "{err}");
    assert!(err.contains("rules"), "{err}");
}
