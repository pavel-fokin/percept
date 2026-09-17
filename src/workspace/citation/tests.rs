use super::*;

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(|line| line.trim_end().to_string()).collect()
}

#[test]
fn found_at_the_top_of_a_file() {
    let lines = lines_of("fn one() {}\nfn two() {}\n");

    assert_eq!(locate_in("fn one() {}", &lines), Some((1, 1)));
}

#[test]
fn found_after_the_text_moved_down() {
    let lines = lines_of("fn zero() {}\nfn one() {}\n");

    assert_eq!(locate_in("fn one() {}", &lines), Some((2, 2)));
}

#[test]
fn not_found_when_the_excerpt_is_gone() {
    let lines = lines_of("fn two() {}\n");

    assert_eq!(locate_in("fn one() {}", &lines), None);
}

#[test]
fn a_multi_line_excerpt_matches_a_contiguous_run() {
    let lines = lines_of("one\ntwo\nthree\nfour\n");

    assert_eq!(locate_in("two\nthree", &lines), Some((2, 3)));
}

#[test]
fn an_excerpt_padded_with_blank_lines_still_matches() {
    let lines = lines_of("one\ntwo\nthree\n");

    assert_eq!(locate_in("\n\ntwo  \n\n", &lines), Some((2, 2)));
}

/// Leading whitespace is part of the line, so an excerpt that lost its
/// indentation does not match the line it came from - which is why a
/// citation records the file's own text rather than a retyped copy.
#[test]
fn an_excerpt_that_lost_its_indentation_is_not_found() {
    let lines = lines_of("fn main() {\n    let x = 1;\n}\n");

    assert_eq!(locate_in("let x = 1;", &lines), None);
}

#[test]
fn an_excerpt_of_nothing_but_blanks_is_not_found() {
    let lines = lines_of("one\ntwo\n");

    assert_eq!(locate_in("   \n  ", &lines), None);
}

#[test]
fn a_files_own_leading_blank_lines_still_count_toward_the_range() {
    let lines = lines_of("\n\nfn one() {}\n");

    assert_eq!(locate_in("fn one() {}", &lines), Some((3, 3)));
}

#[test]
fn a_path_outside_the_checkout_is_gone() {
    let checkout = crate::core::testing::Fixture::new();
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("../escape.rs"), "anything"), Cited::Gone);
}

#[test]
fn a_cited_file_reports_where_its_excerpt_sits_now() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/a.rs", "fn other() {}\nfn one() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("src/a.rs"), "fn one() {}"), Cited::At(2, 2));
}

#[test]
fn a_cited_file_whose_excerpt_is_gone_reads_as_changed() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/a.rs", "fn other() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("src/a.rs"), "fn one() {}"), Cited::Changed);
}

#[test]
fn a_missing_file_whose_excerpt_is_nowhere_else_reads_as_gone() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/b.rs", "fn other() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("src/a.rs"), "fn one() {}"), Cited::Gone);
}

#[test]
fn a_renamed_file_is_found_by_a_text_search_of_the_checkout() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/renamed.rs", "fn one() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(
        citations.locate(Path::new("src/a.rs"), "fn one() {}"),
        Cited::Moved(PathBuf::from("src/renamed.rs"))
    );
}

#[test]
fn a_file_that_still_exists_reads_as_changed_even_when_its_text_sits_elsewhere() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("web/src/theme.css", "body { color: red; }\n");
    checkout.write("docs/copy.css", "a { color: blue; }\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(
        citations.locate(Path::new("web/src/theme.css"), "a { color: blue; }"),
        Cited::Changed,
        "a copy under a file that still exists is not a rename"
    );
}

#[test]
fn an_excerpt_found_at_more_than_one_other_path_is_not_a_move() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/b.rs", "fn one() {}\n");
    checkout.write("src/c.rs", "fn one() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("src/a.rs"), "fn one() {}"), Cited::Gone);
}

#[test]
fn an_excerpt_still_at_its_own_path_is_not_reported_as_moved() {
    let checkout = crate::core::testing::Fixture::new();
    checkout.write("src/a.rs", "fn other() {}\nfn one() {}\n");
    let citations = Citations::new(checkout.path());

    assert_eq!(citations.locate(Path::new("src/a.rs"), "fn one() {}"), Cited::At(2, 2));
}
