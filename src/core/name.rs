use std::path::Path;

use uuid::Uuid;

use crate::shared::Result;

/// The name of a project or a board: lowercase, starting with `a-z` or `0-9`, and only `a-z`, `0-9`, `.`, `_` and `-`.
/// It never reads as an id, so a URL segment holding either is unambiguous.
#[derive(Debug, Clone, PartialEq)]
pub struct Name(String);

impl Name {
    /// Lowercases `name`, so names are case-insensitive, then validates it.
    pub fn new(name: &str) -> Result<Self> {
        let name = name.to_lowercase();
        let starts_well = name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit());
        if !starts_well || !name.chars().all(allowed) {
            return Err(format!("invalid name {name:?}: start with a-z or 0-9, then use a-z, 0-9, '.', '_' and '-'").into());
        }
        if Uuid::try_parse(&name).is_ok() {
            return Err(format!("invalid name {name:?}: it reads as an id").into());
        }

        Ok(Self(name))
    }

    /// The name of a project rooted at `root`: its folder name, lowercased,
    /// each run of other characters replaced by one `-`, and leading `.`, `_`, `-` and trailing `-` trimmed.
    pub fn from_root(root: &Path) -> Result<Self> {
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

        if name.is_empty() {
            return Err("pass a name: percept init <name>".into());
        }

        Self::new(name)
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
    fn a_name_is_lowercased() {
        assert_eq!(Name::new("My-App_1.0").unwrap().as_str(), "my-app_1.0");
    }

    #[test]
    fn a_name_that_is_empty_or_has_other_characters_is_refused() {
        for name in ["", "my app", "a/b", "café", "a:b", ".", "..", ".x", "-x", "_x"] {
            assert!(Name::new(name).is_err(), "{name:?}");
        }
    }

    #[test]
    fn a_name_that_reads_as_an_id_is_refused() {
        for name in ["0190a3b2-7c4e-7d2f-8a1b-1c2d3e4f5a6b", "0190A3B2-7C4E-7D2F-8A1B-1C2D3E4F5A6B", "0190a3b27c4e7d2f8a1b1c2d3e4f5a6b"] {
            let error = Name::new(name).unwrap_err();

            assert!(error.to_string().ends_with("it reads as an id"), "{name:?}");
        }
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
    fn a_folder_with_nothing_usable_in_its_name_needs_a_name_passed() {
        for root in ["/work/日本", "/work/---", "/"] {
            let error = Name::from_root(Path::new(root)).unwrap_err();

            assert_eq!(error.to_string(), "pass a name: percept init <name>", "{root}");
        }
    }
}
