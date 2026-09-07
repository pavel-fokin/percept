use super::*;
use std::fs;

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        Self { dir }
    }

    fn write(&self, path: &str, content: &str) -> &Self {
        let full = self.dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
        self
    }

    fn tool(&self) -> GrepFiles {
        GrepFiles::new(Arc::new(Workspace::new(self.dir.path()).unwrap()))
    }
}

#[test]
fn finds_a_line_in_a_nested_file_with_path_and_line_number() {
    let fixture = Fixture::new();
    fixture.write("src/app/mod.rs", "fn one() {}\nfn needle() {}\n");

    let out = fixture.tool().run(r#"{"pattern": "needle"}"#).unwrap();

    assert_eq!(out.content, "src/app/mod.rs:2:fn needle() {}");
}

#[test]
fn the_glob_filter_narrows_to_one_extension() {
    let fixture = Fixture::new();
    fixture.write("a.rs", "needle\n").write("a.md", "needle\n");

    let out = fixture
        .tool()
        .run(r#"{"pattern": "needle", "glob": "*.rs"}"#)
        .unwrap();

    assert_eq!(out.content, "a.rs:1:needle");
}

#[test]
fn a_binary_file_is_skipped() {
    let fixture = Fixture::new();
    fixture.write("a.rs", "needle\n");
    fs::write(fixture.dir.path().join("b.bin"), [0u8, 1, 2]).unwrap();

    let out = fixture.tool().run(r#"{"pattern": "needle"}"#).unwrap();

    assert_eq!(out.content, "a.rs:1:needle");
}

#[test]
fn a_bad_regex_errors() {
    let fixture = Fixture::new();

    assert!(fixture.tool().run(r#"{"pattern": "("}"#).is_err());
}

#[test]
fn the_cap_trailer_appears_past_200_matches() {
    let fixture = Fixture::new();
    let content: String = (0..210).map(|_| "needle\n").collect();
    fixture.write("a.rs", &content);

    let out = fixture.tool().run(r#"{"pattern": "needle"}"#).unwrap();

    assert!(
        out.content
            .ends_with("[10 more matches; narrow the pattern or path]"),
        "{}",
        out.content
    );
}
