use super::*;

#[test]
fn default_prefix_is_the_name_s_first_letter_lowercased() {
    assert_eq!(default_prefix("Verdict"), "v");
    assert_eq!(default_prefix("chore"), "c");
}

#[test]
fn kind_new_defaults_its_prefix() {
    assert_eq!(NodeKind::new("fact", "g").prefix, "f");
}
