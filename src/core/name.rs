use std::path::Path;

use crate::shared::Result;

/// The name of a project or a board: lowercase, starting with `a-z` or `0-9`, and only `a-z`, `0-9`, `.`, `_` and `-`.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

impl Name {
    /// Trims and lowercases `name`, so names ignore case and surrounding space, then validates it.
    pub fn new(name: &str) -> Result<Self> {
        let name = name.trim().to_lowercase();
        let starts_well = name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit());
        if !starts_well || !name.chars().all(allowed) {
            return Err(format!("invalid name {name:?}: start with a-z or 0-9, then use a-z, 0-9, '.', '_' and '-'").into());
        }

        Ok(Self(name))
    }

    /// The name of a project rooted at `root`: its folder name, lowercased,
    /// each run of other characters replaced by one `-`, and leading `.`, `_`, `-` and trailing `-` trimmed.
    /// `None` when nothing usable is left.
    pub fn from_root(root: &Path) -> Option<Self> {
        let folder = root.file_name().unwrap_or_default().to_string_lossy().to_lowercase();

        let mut name = String::new();
        for c in folder.chars() {
            if allowed(c) {
                name.push(c);
            } else if !name.ends_with('-') {
                name.push('-');
            }
        }
        let name = name.trim_start_matches(['.', '_', '-']).trim_end_matches('-');

        Self::new(name).ok()
    }

    /// The first of `base`, `base-1`, `base-2`, … that `taken` does not hold.
    pub fn free(base: &Name, taken: &[Name]) -> Name {
        let numbered = (1..).map(|n| Name(format!("{}-{n}", base.0)));

        std::iter::once(base.clone()).chain(numbered).find(|name| !taken.contains(name)).expect("an unbounded range has a free name")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn allowed(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_lowercased() {
        assert_eq!(Name::new(" My-App_1.0\n").unwrap().as_str(), "my-app_1.0");
    }

    #[test]
    fn a_name_that_is_empty_or_has_other_characters_is_refused() {
        for name in ["", "my app", "a/b", "café", "a:b", ".", "..", ".x", "-x", "_x"] {
            assert!(Name::new(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn a_free_name_is_the_base_then_the_first_unused_number() {
        let name = |s: &str| Name::new(s).unwrap();
        let base = name("board");

        let free = |taken: &[&str]| Name::free(&base, &taken.iter().map(|s| name(s)).collect::<Vec<_>>()).as_str().to_string();

        assert_eq!(free(&[]), "board");
        assert_eq!(free(&["board"]), "board-1");
        assert_eq!(free(&["board", "board-1"]), "board-2");
        assert_eq!(free(&["board", "board-2"]), "board-1");
        assert_eq!(free(&["board-1"]), "board");
    }

    #[test]
    fn the_default_name_is_the_folder_cleaned_up() {
        let cases = [
            ("/work/My App", "my-app"),
            ("/work/percept", "percept"),
            ("/work/a  b!!c", "a-b-c"),
            ("/work/--x--", "x"),
            ("/work/v1.2_beta", "v1.2_beta"),
            ("/work/café", "caf"),
            ("/work/.dotfiles", "dotfiles"),
            ("/work/_-.x", "x"),
        ];

        for (root, name) in cases {
            assert_eq!(Name::from_root(Path::new(root)).unwrap().as_str(), name, "{root}");
        }
    }

    #[test]
    fn a_folder_with_nothing_usable_in_its_name_has_no_default() {
        for root in ["/work/日本", "/work/---", "/"] {
            assert_eq!(Name::from_root(Path::new(root)), None, "{root}");
        }
    }
}
