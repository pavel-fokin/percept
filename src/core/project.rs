use std::path::{Path, PathBuf};

use super::{Event, Payload};
use crate::shared::{Id, Result};

pub type ProjectId = Id<Project>;

/// A folder percept tracks, created by a `ProjectCreated` event.
/// Its root is the `source.path` of that event.
pub struct Project {
    pub id: ProjectId,
    pub name: ProjectName,
    pub root: PathBuf,
}

impl Project {
    /// Every project in `events` in `ProjectCreated` order.
    pub fn all(events: &[Event]) -> Vec<Self> {
        events
            .iter()
            .filter_map(|event| match &event.payload {
                Payload::ProjectCreated { project, name } => {
                    Some(Self { id: *project, name: name.clone(), root: event.source.path.clone() })
                }
                _ => None,
            })
            .collect()
    }

    /// The project whose root is the nearest at or above `path`, as git finds its root.
    pub fn containing<'a>(projects: &'a [Self], path: &Path) -> Option<&'a Self> {
        projects.iter().filter(|p| path.starts_with(&p.root)).max_by_key(|p| p.root.components().count())
    }
}

/// A project's name: lowercase, nonempty, and only `a-z`, `0-9`, `.`, `_` and `-`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectName(String);

impl ProjectName {
    /// Lowercases `name`, so names are case-insensitive, then validates it.
    pub fn new(name: &str) -> Result<Self> {
        let name = name.to_lowercase();
        if name.is_empty() || !name.chars().all(allowed) {
            return Err(format!("invalid project name {name:?}: use a-z, 0-9, '.', '_' and '-'").into());
        }

        Ok(Self(name))
    }

    /// The name of a project rooted at `root`: its folder name, lowercased,
    /// each run of other characters replaced by one `-`, and outer `-` trimmed.
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
        let name = name.trim_matches('-');

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
    use crate::core::{Actor, Source};

    #[test]
    fn a_name_is_lowercased() {
        assert_eq!(ProjectName::new("My-App_1.0").unwrap().as_str(), "my-app_1.0");
    }

    #[test]
    fn a_name_that_is_empty_or_has_other_characters_is_refused() {
        for name in ["", "my app", "a/b", "café", "a:b"] {
            assert!(ProjectName::new(name).is_err(), "{name:?}");
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
        ];

        for (root, name) in cases {
            assert_eq!(ProjectName::from_root(Path::new(root)).unwrap().as_str(), name, "{root}");
        }
    }

    #[test]
    fn a_folder_with_nothing_usable_in_its_name_needs_a_name_passed() {
        for root in ["/work/日本", "/work/---", "/"] {
            let error = ProjectName::from_root(Path::new(root)).unwrap_err();

            assert_eq!(error.to_string(), "pass a name: percept init <name>", "{root}");
        }
    }

    #[test]
    fn the_nearest_root_at_or_above_a_path_wins() {
        let source = |path: &str| Source { client: "percept-cli".into(), path: path.into() };
        let events = [
            Event::project_created(source("/work/app"), Actor::Human, ProjectId::new(), ProjectName::new("app").unwrap()),
            Event::project_created(source("/work/app/inner"), Actor::Human, ProjectId::new(), ProjectName::new("inner").unwrap()),
            Event::project_created(source("/work/apple"), Actor::Human, ProjectId::new(), ProjectName::new("apple").unwrap()),
        ];
        let projects = Project::all(&events);

        let name = |path: &str| Project::containing(&projects, Path::new(path)).map(|p| p.name.as_str());

        assert_eq!(name("/work/app"), Some("app"));
        assert_eq!(name("/work/app/src/deep"), Some("app"));
        assert_eq!(name("/work/app/inner/src"), Some("inner"));
        assert_eq!(name("/work/apple/x"), Some("apple"));
        assert_eq!(name("/work"), None);
        assert_eq!(name("/elsewhere"), None);
    }

    #[test]
    fn projects_are_folded_from_their_events_in_log_order() {
        let source = |path: &str| Source { client: "percept-cli".into(), path: path.into() };
        let events = [
            Event::project_created(source("/a"), Actor::Human, ProjectId::new(), ProjectName::new("a").unwrap()),
            Event::map_created(Source::fixture(), Actor::Human, crate::core::MapId::new(), ProjectId::new(), "decisions"),
            Event::project_created(source("/b"), Actor::Human, ProjectId::new(), ProjectName::new("b").unwrap()),
        ];

        let projects = Project::all(&events);

        let found: Vec<(&str, &Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_path())).collect();
        assert_eq!(found, vec![("a", Path::new("/a")), ("b", Path::new("/b"))]);
    }
}
