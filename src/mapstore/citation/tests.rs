use super::*;

#[test]
fn found_at_the_top_of_a_file() {
    let text = "fn one() {}\nfn two() {}\n";

    assert_eq!(locate("fn one() {}", text), Some((1, 1)));
}

#[test]
fn found_after_the_text_moved_down() {
    let text = "fn zero() {}\nfn one() {}\n";

    assert_eq!(locate("fn one() {}", text), Some((2, 2)));
}

#[test]
fn not_found_when_the_excerpt_is_gone() {
    let text = "fn two() {}\n";

    assert_eq!(locate("fn one() {}", text), None);
}

#[test]
fn a_single_line_excerpt_matches_its_own_line() {
    let text = "one\ntwo\nthree\n";

    assert_eq!(locate("two", text), Some((2, 2)));
}

#[test]
fn an_excerpt_that_normalises_to_nothing_is_not_found() {
    let text = "one\ntwo\n";

    assert_eq!(locate("   \n  ", text), None);
}

#[test]
fn a_multi_line_excerpt_matches_a_contiguous_run() {
    let text = "one\ntwo\nthree\nfour\n";

    assert_eq!(locate("two\nthree", text), Some((2, 3)));
}
