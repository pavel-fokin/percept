use std::path::Path;

/// A path written from `~` when it is home or under it.
pub(super) fn display_path(path: &Path, home: &Path) -> String {
    display_paths(&path.display().to_string(), home)
}

/// Every path in `text` that starts at home written from `~`, so a command reads like a file path does.
/// Home counts only as a whole path segment that begins a word: `/mnt/Users/me/a` and `/Users/me2` stay.
/// A home of `/` or empty is no home: everything would sit under it.
pub(super) fn display_paths(text: &str, home: &Path) -> String {
    if home.parent().is_none() {
        return text.to_string();
    }
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

    #[test]
    fn paths_under_home_inside_text_start_with_a_tilde() {
        let text = "cd /Users/pavel-fokin && sed -n 1,9p '/Users/pavel-fokin/a.rs' /Users/pavel-fokinx/b.rs";

        let shown = display_paths(text, Path::new(HOME));

        assert_eq!(shown, "cd ~ && sed -n 1,9p '~/a.rs' /Users/pavel-fokinx/b.rs");
    }

    #[test]
    fn a_path_under_home_starts_with_a_tilde() {
        assert_eq!(display_path(Path::new("/Users/pavel-fokin/code/repo"), Path::new(HOME)), "~/code/repo");
        assert_eq!(display_path(Path::new("/srv/repo"), Path::new(HOME)), "/srv/repo");
    }

    #[test]
    fn home_inside_another_path_stays() {
        let text = "ls /Volumes/backup/Users/pavel-fokin/a.rs";

        assert_eq!(display_paths(text, Path::new(HOME)), text);
    }

    #[test]
    fn a_root_or_empty_home_leaves_text_unchanged() {
        assert_eq!(display_paths("cat /a.rs", Path::new("/")), "cat /a.rs");
        assert_eq!(display_paths("cat /a.rs", Path::new("")), "cat /a.rs");
    }
}
