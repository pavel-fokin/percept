use super::*;
use crate::core::testing::Fixture;

#[test]
fn a_project_with_no_schemas_directory_has_no_maps() {
    let fixture = Fixture::new();
    let schemas = load(fixture.path()).unwrap();
    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn each_shipped_template_parses_under_its_own_name() {
    for (name, text) in templates() {
        let schema = parse(&name, text).unwrap();
        assert_eq!(schema.name, name);
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

    assert_eq!(covers.from, ["concept"]);
    assert_eq!(covers.to, ["concept"]);
}

#[test]
fn a_project_schema_of_a_new_name_is_added() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\n\
         purpose = \"terms and their meaning\"\n\
         \n\
         [[node]]\n\
         kind = \"term\"\n\
         gloss = \"a word this project uses in a specific way\"\n\
         requires = [\"meaning\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["glossary"]);
    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().requires, ["meaning"]);
}

#[test]
fn a_toml_syntax_error_names_the_file() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/broken.toml", "name = \"broken\npurpose =");

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("broken.toml:"), "{err}");
    assert!(err.contains("line"), "{err}");
}

#[test]
fn a_name_that_does_not_match_the_file_stem_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"terms\"\npurpose = \"p\"\n\n[[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: declares name \"terms\", which does not match the file name"
    );
}

#[test]
fn a_schema_with_no_node_kinds_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(err, "glossary.toml: declares no node kinds");
}

#[test]
fn a_schema_that_still_declares_headlines_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\nheadlines = [\"term\"]\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml: a schema no longer declares `headlines`"), "{err}");
}

#[test]
fn a_settles_key_is_an_unknown_field() {
    // The core keeps no settlement pair - "settles" is not a shape any
    // schema file declares.
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\
         settles = { by = \"resolution\", of = \"term\" }\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n",
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

    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn a_file_with_an_unknown_key_is_refused_naming_the_key() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\nheadline = [\"term\"]\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("headline"), "{err}");
}

#[test]
fn a_kind_declared_with_name_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n[[node]]\nname = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("name"), "{err}");
}

#[test]
fn a_blank_kind_name_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n[[node]]\nkind = \"\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("glossary.toml:"), "{err}");
    assert!(err.contains("blank"), "{err}");
}

#[test]
fn a_duplicate_node_kind_name_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g2\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("twice"), "{err}");
}

#[test]
fn a_kind_with_no_gloss_loads_with_an_empty_gloss() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n[[node]]\nkind = \"term\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().gloss, "");
}

#[test]
fn a_node_kind_with_no_prefix_defaults_to_its_first_letter() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n[[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().prefix, "t");
}

#[test]
fn a_node_kind_with_an_explicit_prefix_keeps_it() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nprefix = \"tm\"\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().prefix, "tm");
}

#[test]
fn two_node_kinds_defaulting_to_the_same_prefix_are_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[node]]\nkind = \"taxonomy\"\ngloss = \"g2\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: node kinds \"term\" and \"taxonomy\" both take the short id prefix \"t\""
    );
}

#[test]
fn an_explicit_prefix_colliding_with_a_default_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[node]]\nkind = \"acronym\"\ngloss = \"g2\"\nprefix = \"t\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: node kinds \"term\" and \"acronym\" both take the short id prefix \"t\""
    );
}

#[test]
fn a_blank_requires_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nrequires = [\"\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("blank property"), "{err}");
}

#[test]
fn a_states_list_loads_onto_the_node_kind() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstates = [\"open\", \"done\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().states, ["open", "done"]);
}

#[test]
fn the_old_state_key_is_refused_with_the_rename() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstate = [\"open\", \"done\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(
        err.contains("schema key `state` was renamed to `states`"),
        "{err}"
    );
}

#[test]
fn a_states_list_of_one_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstates = [\"open\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("fewer than two states"), "{err}");
}

#[test]
fn a_blank_states_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstates = [\"open\", \"\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("blank state"), "{err}");
}

#[test]
fn a_repeated_states_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstates = [\"open\", \"open\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("twice"), "{err}");
}

#[test]
fn an_edge_end_naming_an_undeclared_kind_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[edge]]\nkind = \"relates\"\ngloss = \"g\"\nfrom = \"term\"\nto = \"acronym\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: edge kind \"relates\"'s to names \"acronym\", which is not a declared \
         node kind"
    );
}

#[test]
fn an_edge_end_with_no_kinds_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[edge]]\nkind = \"relates\"\ngloss = \"g\"\nfrom = \"term\"\nto = []\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(err, "glossary.toml: edge kind \"relates\"'s to names no node kind");
}

#[test]
fn an_edge_end_naming_a_list_loads_both_kinds() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [[node]]\nkind = \"acronym\"\ngloss = \"g\"\n\n\
         [[edge]]\nkind = \"relates\"\ngloss = \"g\"\nfrom = \"term\"\nto = [\"term\", \
         \"acronym\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    let relates = glossary.edge_kind("relates").unwrap();
    assert_eq!(relates.to, ["term".to_string(), "acronym".to_string()]);
}

#[test]
fn a_properties_list_loads_onto_the_node_kind() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nproperties = [\"note\", \"summary\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(
        glossary.node_kind("term").unwrap().properties,
        ["note", "summary"]
    );
}

#[test]
fn a_property_listed_in_both_requires_and_properties_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nrequires = [\"meaning\"]\n\
         properties = [\"meaning\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: node kind \"term\" declares \"meaning\" in both requires and properties"
    );
}

#[test]
fn state_under_properties_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nproperties = [\"state\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("states = [...] only"), "{err}");
}

#[test]
fn state_under_requires_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nrequires = [\"state\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("states = [...] only"), "{err}");
}

#[test]
fn a_rules_table_loads_lines_for_each_declared_moment() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [rules]\n\"session.started\" = [\"s\"]\n\"message.received\" = [\"a\", \"b\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.rules.at("session.started"), ["s"]);
    assert_eq!(glossary.rules.at("message.received"), ["a", "b"]);
}

#[test]
fn reflection_started_lines_load_onto_the_schema() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [rules]\n\"reflection.started\" = [\"r\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.rules.at("reflection.started"), ["r"]);
}

#[test]
fn a_blank_line_under_any_moment_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [rules]\n\"message.received\" = [\"\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("glossary.toml"), "{err}");
    assert!(err.contains("blank \"message.received\" entry"), "{err}");
}

#[test]
fn a_key_outside_the_three_moments_is_refused_naming_them() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n\n\
         [rules]\nturn = [\"a\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("glossary.toml"), "{err}");
    assert!(err.contains("\"turn\""), "{err}");
    assert!(err.contains("session.started"), "{err}");
    assert!(err.contains("message.received"), "{err}");
    assert!(err.contains("reflection.started"), "{err}");
}

#[test]
fn the_decisions_template_carries_message_received_rules() {
    let (name, text) = templates().into_iter().find(|(name, _)| name == "concepts").unwrap();
    let schema = parse(&name, text).unwrap();

    assert_eq!(schema.rules.at("message.received").len(), 2);
}

#[test]
fn the_decisions_template_carries_reflection_started_rules() {
    let (name, text) = templates().into_iter().find(|(name, _)| name == "concepts").unwrap();
    let schema = parse(&name, text).unwrap();

    assert_eq!(schema.rules.at("reflection.started").len(), 5);
}
