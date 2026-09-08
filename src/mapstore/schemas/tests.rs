use super::*;
use crate::core::testing::{decisions, tasks, Fixture};

#[test]
fn the_embedded_decisions_toml_folds_to_the_decisions_fixture() {
    let schema = parse("decisions.toml", DECISIONS_TOML).unwrap();
    assert_eq!(schema, decisions());
}

#[test]
fn the_embedded_tasks_toml_folds_to_the_tasks_fixture() {
    let schema = parse("tasks.toml", TASKS_TOML).unwrap();
    assert_eq!(schema, tasks());
}

#[test]
fn a_project_with_no_schemas_directory_has_only_the_built_ins() {
    let fixture = Fixture::new();
    let schemas = load(fixture.path()).unwrap();
    let names: Vec<&str> = schemas.folded().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "tasks"]);
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
         [[nodes]]\n\
         name = \"term\"\n\
         gloss = \"a word this project uses in a specific way\"\n\
         requires = [\"meaning\"]\n",
    );

    let schemas = load(fixture.path()).unwrap();

    let names: Vec<&str> = schemas.folded().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "tasks", "glossary"]);
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

    let names: Vec<&str> = schemas.folded().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decisions", "tasks"], "the built-in's slot, not appended");
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
        "name = \"terms\"\npurpose = \"p\"\n\n[[nodes]]\nname = \"term\"\ngloss = \"g\"\n",
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
         [[nodes]]\nname = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: headlines names \"acronym\", which is not a declared node kind"
    );
}

#[test]
fn settles_naming_an_undeclared_kind_is_refused() {
    let fixture = Fixture::new();
    fixture.write(
        ".percept/schemas/glossary.toml",
        "name = \"glossary\"\npurpose = \"p\"\n\
         settles = { by = \"resolution\", of = \"term\" }\n\n\
         [[nodes]]\nname = \"term\"\ngloss = \"g\"\n",
    );

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "glossary.toml: settles.by names \"resolution\", which is not a declared node kind"
    );
}

#[test]
fn a_file_named_code_is_refused() {
    let fixture = Fixture::new();
    fixture.write(".percept/schemas/code.toml", "name = \"code\"\npurpose = \"p\"\n");

    let err = load(fixture.path()).err().unwrap().to_string();

    assert_eq!(
        err,
        "code.toml: code is derived from the working tree, not declared"
    );
}
