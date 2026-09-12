use super::*;
use crate::core::testing::{concepts, decisions, Fixture};

#[test]
fn the_embedded_decisions_toml_folds_to_the_decisions_fixture() {
    let schema = parse("decisions", DECISIONS_TOML).unwrap();
    assert_eq!(schema, decisions());
}

#[test]
fn the_embedded_concepts_toml_folds_to_the_concepts_fixture() {
    let schema = parse("concepts", CONCEPTS_TOML).unwrap();
    assert_eq!(schema, concepts());
}

#[test]
fn a_project_with_no_schemas_directory_has_only_the_built_ins() {
    let fixture = Fixture::new();
    let schemas = load(fixture.path()).unwrap();
    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "concepts"]);
}

#[test]
fn a_project_schema_of_a_new_name_is_added() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\n\
         purpose = \"terms and their meaning\"\n\
         headlines = [\"term\"]\n\
         \n\
         [[node]]\n\
         kind = \"term\"\n\
         gloss = \"a word this project uses in a specific way\"\n\
         requires = [\"meaning\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "concepts", "glossary"]);
    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().requires, ["meaning"]);
}

#[test]
fn a_project_file_named_for_a_built_in_replaces_it() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/decisions.toml",
        &DECISIONS_TOML.replacen(
            "what was asked, what was chosen, and why, so a settled question is not reopened",
            "a changed purpose",
            1,
        ),
    );

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "concepts"], "the built-in's slot, not appended");
    assert_eq!(schemas.find("decisions").unwrap().purpose, "a changed purpose");
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
fn headlines_naming_an_undeclared_kind_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\nheadlines = [\"acronym\"]\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: headlines names \"acronym\", which is not a declared node kind"
    );
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
fn a_built_in_replacement_that_drops_a_kind_is_refused() {
    let fixture = Fixture::new();
    let dropped = DECISIONS_TOML.replacen(
        "[[node]]\nkind = \"evidence\"\ngloss = \"a fact that supports or contradicts an \
         option\"\n\n",
        "",
        1,
    );
    fixture.write(".percept/schemas/decisions.toml", &dropped);

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("decisions.toml:"), "{err}");
    assert!(err.contains("evidence"), "{err}");
}

#[test]
fn a_built_in_replacement_that_keeps_every_kind_and_adds_one_loads() {
    let fixture = Fixture::new();
    let extended = format!(
        "{DECISIONS_TOML}\n[[node]]\nkind = \"goal\"\ngloss = \"what the project is trying to \
         reach\"\n"
    );
    fixture.write(".percept/schemas/decisions.toml", &extended);

    let schemas = load(fixture.path()).unwrap();

    let decisions = schemas.find("decisions").unwrap();
    assert!(decisions.node_kind("goal").is_some());
    assert!(decisions.node_kind("evidence").is_some());
}

#[test]
fn a_built_in_replacement_that_changes_headlines_is_refused() {
    let fixture = Fixture::new();
    let changed = DECISIONS_TOML.replacen(
        "headlines = [\"question\", \"decision\"]",
        "headlines = [\"question\"]",
        1,
    );
    fixture.write(".percept/schemas/decisions.toml", &changed);

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("decisions.toml:"), "{err}");
    assert!(err.contains("headlines"), "{err}");
}

#[test]
fn a_directory_named_dot_toml_is_ignored() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path().join(".percept/schemas/x.toml")).unwrap();

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "concepts"]);
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
fn a_duplicate_headline_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\nheadlines = [\"term\", \"term\"]\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("twice"), "{err}");
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
fn a_state_list_loads_onto_the_node_kind() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstate = [\"open\", \"done\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let glossary = schemas.find("glossary").unwrap();
    assert_eq!(glossary.node_kind("term").unwrap().states, ["open", "done"]);
}

#[test]
fn a_state_list_of_one_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstate = [\"open\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("fewer than two states"), "{err}");
}

#[test]
fn a_blank_state_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstate = [\"open\", \"\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("blank state"), "{err}");
}

#[test]
fn a_repeated_state_entry_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\n\
         [[node]]\nkind = \"term\"\ngloss = \"g\"\nstate = [\"open\", \"open\"]\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.contains("twice"), "{err}");
}

#[test]
fn a_built_in_replacement_that_changes_a_kind_s_states_is_refused() {
    let fixture = Fixture::new();
    let changed = DECISIONS_TOML.replacen(
        "state = [\"open\", \"answered\", \"dropped\"]",
        "state = [\"open\", \"answered\"]",
        1,
    );
    fixture.write(".percept/schemas/decisions.toml", &changed);

    let err = load(fixture.path()).err().unwrap().to_string();

    assert!(err.starts_with("decisions.toml:"), "{err}");
    assert!(err.contains("states"), "{err}");
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
