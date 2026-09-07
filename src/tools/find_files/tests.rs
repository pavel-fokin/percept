use super::*;
use crate::testing::Fixture;

fn tool(fixture: &Fixture) -> FindFiles {
    FindFiles::new(Arc::new(Workspace::new(fixture.path()).unwrap()))
}

#[test]
fn matches_recursively() {
    let fixture = Fixture::new();
    fixture
        .write("src/main.rs", "")
        .write("src/app/mod.rs", "")
        .write("README.md", "");

    let out = tool(&fixture).run(r#"{"pattern": "src/**/*.rs"}"#).unwrap();

    assert_eq!(out.content, "src/app/mod.rs\nsrc/main.rs");
}

#[test]
fn a_gitignored_file_is_skipped() {
    let fixture = Fixture::new();
    fixture
        .write(".gitignore", "ignored.rs\n")
        .write("src/ignored.rs", "")
        .write("src/kept.rs", "");

    let out = tool(&fixture).run(r#"{"pattern": "src/**/*.rs"}"#).unwrap();

    assert_eq!(out.content, "src/kept.rs");
}

#[test]
fn the_cap_trailer_appears_past_500() {
    let fixture = Fixture::new();
    for i in 0..510 {
        fixture.write(&format!("src/file_{i}.rs"), "");
    }

    let out = tool(&fixture).run(r#"{"pattern": "src/**/*.rs"}"#).unwrap();

    assert!(
        out.content
            .ends_with("[10 more matches; narrow the pattern]"),
        "{}",
        out.content
    );
    assert_eq!(out.content.lines().count(), 501);
}

#[test]
fn a_bad_glob_errors() {
    let fixture = Fixture::new();

    assert!(tool(&fixture).run(r#"{"pattern": "["}"#).is_err());
}
