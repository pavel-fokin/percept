use std::path::Path;

/// A home of `/` or empty is no home: everything would sit under it.
fn real_home(home: &Path) -> Option<&Path> {
    home.parent().map(|_| home)
}

/// `~/rest` under home, the absolute path otherwise.
pub(super) fn display_path(path: &Path, home: &Path) -> String {
    let rest = real_home(home).and_then(|home| path.strip_prefix(home).ok());

    match rest {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Every path in `text` that starts at home written from `~`, so a command reads like a file path does.
/// Home counts only as a whole path segment that begins a word: `/mnt/Users/me/a` and `/Users/me2` stay.
pub(super) fn display_paths(text: &str, home: &Path) -> String {
    let Some(home) = real_home(home) else { return text.to_string() };
    let home = home.display().to_string();

    let mut shown = String::with_capacity(text.len());
    let mut copied = 0;
    for (at, _) in text.match_indices(&home) {
        let end = at + home.len();
        let starts_word = text[..at].chars().next_back().is_none_or(|c| c.is_whitespace() || "'\"=:(".contains(c));
        let ends_segment = text[end..].chars().next().is_none_or(|c| c == '/' || c.is_whitespace() || "'\";:)".contains(c));
        if starts_word && ends_segment {
            shown.push_str(&text[copied..at]);
            shown.push('~');
            copied = end;
        }
    }
    shown.push_str(&text[copied..]);

    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/pavel-fokin";

    fn shown(path: &str) -> String {
        display_path(Path::new(path), Path::new(HOME))
    }

    #[test]
    fn home_itself_is_a_tilde() {
        assert_eq!(shown("/Users/pavel-fokin"), "~");
    }

    #[test]
    fn a_path_under_home_starts_with_a_tilde() {
        assert_eq!(shown("/Users/pavel-fokin/code/percept"), "~/code/percept");
    }

    #[test]
    fn a_path_outside_home_stays_absolute() {
        assert_eq!(shown("/work/app"), "/work/app");
    }

    #[test]
    fn a_sibling_of_home_is_outside_it() {
        assert_eq!(shown("/Users/pavel-fokinX/code"), "/Users/pavel-fokinX/code");
    }

    #[test]
    fn paths_under_home_inside_text_start_with_a_tilde() {
        let text = "cd /Users/pavel-fokin && sed -n 1,9p '/Users/pavel-fokin/a.rs' /Users/pavel-fokinx/b.rs";

        let shown = display_paths(text, Path::new(HOME));

        assert_eq!(shown, "cd ~ && sed -n 1,9p '~/a.rs' /Users/pavel-fokinx/b.rs");
    }

    #[test]
    fn home_inside_another_path_stays() {
        let text = "ls /Volumes/backup/Users/pavel-fokin/a.rs";

        assert_eq!(display_paths(text, Path::new(HOME)), text);
    }

    #[test]
    fn a_root_home_leaves_text_unchanged() {
        assert_eq!(display_paths("cat /a.rs", Path::new("/")), "cat /a.rs");
    }

    #[test]
    fn a_root_or_empty_home_is_no_home() {
        assert_eq!(display_path(Path::new("/work/app"), Path::new("/")), "/work/app");
        assert_eq!(display_path(Path::new("/work/app"), Path::new("")), "/work/app");
    }
}
