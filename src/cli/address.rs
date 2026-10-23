//! Where the CLI names a project, a board or a map: parsing the paths users type and printing the paths of what exists.

use std::fmt;
use std::path::Path;

use crate::app::{AppService, BoardContents, source_path};
use crate::core::{EventStore, Map, Name, Place, Project, SkillStore};
use crate::shared::Result;

/// A bare name, for a map of the current project, or an absolute path of one to three segments:
/// `/<project>`, `/<project>/<map>`, `/<project>/<board>`, `/<project>/<board>/<map>`.
#[derive(Debug, PartialEq)]
pub(super) enum Address {
    Bare(String),
    Absolute(Vec<String>),
}

impl Address {
    pub(super) fn parse(text: &str) -> Result<Self> {
        let absolute = text.starts_with('/');
        let segments: Vec<&str> = text.strip_prefix('/').unwrap_or(text).split('/').collect();

        let bad = segments.iter().any(|s| s.is_empty() || *s == "." || *s == "..");
        if bad || segments.len() > 3 || (!absolute && segments.len() > 1) {
            return Err(format!("{text:?} is not a map name or an absolute path like /<project>/<map>").into());
        }

        Ok(if absolute { Self::Absolute(segments.into_iter().map(String::from).collect()) } else { Self::Bare(text.to_string()) })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bare(name) => f.write_str(name),
            Self::Absolute(segments) => write!(f, "/{}", segments.join("/")),
        }
    }
}

/// What `show` lists or prints.
pub(super) enum Target {
    Project(Project),
    Board(BoardContents),
    Map(Map),
}

/// The project the checkout at the current directory belongs to.
pub(super) async fn project_of<S: EventStore, M: SkillStore>(service: &AppService<S, M>, path: &Path) -> Result<Project> {
    Ok(service.project_at(path).await?.ok_or("not in a project; run percept init")?)
}

/// The map `address` names. A bare name is looked up in the current checkout's project, wherever the map is shown.
pub(super) async fn map<S: EventStore, M: SkillStore>(service: &AppService<S, M>, address: &Address) -> Result<Map> {
    match address {
        Address::Bare(name) => {
            let project = project_of(service, &source_path(&std::env::current_dir()?).await).await?;
            let map = service.map_id(project.name.as_str(), name).await?.ok_or_else(|| format!("this project has no {name} map"))?;

            service.map(map).await
        }
        Address::Absolute(segments) => match segments.as_slice() {
            [project, map] => in_project(service, address, project, map).await,
            [project, board, map] => on_board(service, address, project, board, map).await,
            _ => Err(format!("{address} is not a map").into()),
        },
    }
}

/// What `show` lists or prints for `address`; none means the current checkout's project.
pub(super) async fn target<S: EventStore, M: SkillStore>(service: &AppService<S, M>, address: Option<&Address>) -> Result<Target> {
    match address {
        None => Ok(Target::Project(project_of(service, &source_path(&std::env::current_dir()?).await).await?)),
        Some(Address::Absolute(segments)) if segments.len() == 1 => Ok(Target::Project(project_named(service, &segments[0]).await?)),
        Some(Address::Absolute(segments)) if segments.len() == 2 => {
            let address = address.expect("matched Some");
            let board = service.board_contents(&segments[0], &segments[1]).await?;
            let map = in_project(service, address, &segments[0], &segments[1]).await;

            match (board, map) {
                (Some(_), Ok(_)) => Err(format!("{address} is both a board and a map").into()),
                (Some(board), Err(_)) => Ok(Target::Board(board)),
                (None, map) => map.map(Target::Map),
            }
        }
        Some(address) => map(service, address).await.map(Target::Map),
    }
}

/// The path of `map`: in its project's map list when it is shown there, else on the first board it is shown on.
pub(super) async fn of<S: EventStore, M: SkillStore>(service: &AppService<S, M>, map: &Map) -> Result<String> {
    let project = service.projects().await?.into_iter().find(|p| p.id == map.project).ok_or("the map's project is missing")?;

    if map.places.contains(Place::Project(map.project)) {
        return Ok(format!("/{}/{}", project.name.as_str(), map.name.as_str()));
    }

    let boards = service.boards().await?;
    let board = map.places.iter().find_map(|place| match place {
        Place::Board(id) => boards.iter().find(|b| b.id == id),
        Place::Project(_) => None,
    });

    Ok(format!("/{}/{}/{}", project.name.as_str(), board.ok_or("the map is shown nowhere")?.name.as_str(), map.name.as_str()))
}

async fn project_named<S: EventStore, M: SkillStore>(service: &AppService<S, M>, name: &str) -> Result<Project> {
    Ok(service.project_named(name).await?.ok_or_else(|| format!("no project {name}"))?)
}

async fn in_project<S: EventStore, M: SkillStore>(service: &AppService<S, M>, address: &Address, project: &str, map: &str) -> Result<Map> {
    let project = project_named(service, project).await?;
    let name = Name::new(map).map_err(|_| format!("no map at {address}"))?;

    let maps = service.maps(project.id).await?;

    Ok(maps.into_iter().find(|m| m.name == name).ok_or_else(|| format!("no map at {address}"))?)
}

async fn on_board<S: EventStore, M: SkillStore>(service: &AppService<S, M>, address: &Address, project: &str, board: &str, map: &str) -> Result<Map> {
    project_named(service, project).await?;
    let contents = service.board_contents(project, board).await?.ok_or_else(|| format!("no board /{project}/{board}"))?;
    let name = Name::new(map).map_err(|_| format!("no map at {address}"))?;

    Ok(contents.maps.into_iter().find(|m| m.name == name).ok_or_else(|| format!("no map at {address}"))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Actor, BoardId, Name as N, ClientSessionId, Event, MapId, Places, ProjectId, SessionId, Skill, Source};

    struct Log(Vec<Event>);

    impl EventStore for Log {
        type Lock = ();

        async fn lock(&self) -> Result<()> {
            Ok(())
        }

        async fn session(&self, _: &ClientSessionId) -> Result<Option<SessionId>> {
            Ok(None)
        }

        async fn all(&self) -> Result<Vec<Event>> {
            Ok(self.0.clone())
        }

        async fn append(&self, _: &Event) -> Result<()> {
            unimplemented!()
        }
    }

    struct NoSkills;

    impl SkillStore for NoSkills {
        async fn skill(&self, _: &Path) -> Result<Skill> {
            unimplemented!()
        }

        async fn skills(&self, _: &Path) -> Result<Vec<(String, Result<Skill>)>> {
            unimplemented!()
        }
    }

    fn project(name: &str) -> (ProjectId, Event) {
        let id = ProjectId::new();

        (id, Event::project_created(Source::fixture(), Actor::Human, id, N::new(name).unwrap(), None))
    }

    fn created(project: ProjectId, name: &str, places: Places) -> Event {
        Event::map_created(Source::fixture(), Actor::Human, MapId::new(), project, N::new(name).unwrap(), "/skills/s".into(), places)
    }

    /// Projects repo and other; repo has the board ideas and the maps d (listed), shelf (on ideas only), both
    /// (listed and on ideas) and ideas (listed, named like the board); other has d.
    fn service() -> AppService<Log, NoSkills> {
        let (repo, created_repo) = project("repo");
        let (other, created_other) = project("other");
        let board = BoardId::new();
        let listed = Places::fixture(repo);
        let on_board = Places::new([Place::Board(board)]).unwrap();
        let events = vec![
            created_repo,
            created_other,
            Event::board_created(Source::fixture(), Actor::Human, board, repo, N::new("ideas").unwrap()),
            created(repo, "d", listed.clone()),
            created(repo, "shelf", on_board),
            created(repo, "both", listed.with(Place::Board(board))),
            created(repo, "ideas", listed),
            created(other, "d", Places::fixture(other)),
        ];

        AppService::new(Log(events), NoSkills)
    }

    fn address(text: &str) -> Address {
        Address::parse(text).unwrap()
    }

    #[test]
    fn a_bare_name_and_an_absolute_path_parse() {
        assert_eq!(address("d"), Address::Bare("d".into()));
        assert_eq!(address("/repo"), Address::Absolute(vec!["repo".into()]));
        assert_eq!(address("/repo/d"), Address::Absolute(vec!["repo".into(), "d".into()]));
        assert_eq!(address("/repo/ideas/d").to_string(), "/repo/ideas/d");
    }

    #[test]
    fn relative_dotted_empty_and_long_paths_are_errors() {
        for text in ["repo/d", "./d", "..", ".", "", "/", "/repo/", "//d", "/repo/../d", "/repo/./d", "/a/b/c/d", "a/"] {
            assert!(Address::parse(text).is_err(), "accepted {text:?}");
        }
    }

    #[tokio::test]
    async fn a_map_is_found_by_project_path_and_by_board_path() {
        let service = service();

        let listed = map(&service, &address("/repo/d")).await.ok().unwrap();
        let other = map(&service, &address("/OTHER/d")).await.ok().unwrap();
        let on_board = map(&service, &address("/repo/ideas/shelf")).await.ok().unwrap();
        let both = map(&service, &address("/repo/ideas/both")).await.ok().unwrap();

        assert_eq!(listed.name.as_str(), "d");
        assert_ne!(listed.id, other.id);
        assert_eq!(on_board.name.as_str(), "shelf");
        assert_eq!(both.name.as_str(), "both");
    }

    #[tokio::test]
    async fn a_path_must_match_where_the_map_is_shown() {
        let service = service();

        let on_board_only = map(&service, &address("/repo/shelf")).await.err().unwrap();
        let not_on_board = map(&service, &address("/repo/ideas/d")).await.err().unwrap();

        assert_eq!(on_board_only.to_string(), "no map at /repo/shelf");
        assert_eq!(not_on_board.to_string(), "no map at /repo/ideas/d");
    }

    #[tokio::test]
    async fn unknown_projects_boards_and_maps_are_errors() {
        let service = service();

        let error = async |text| map(&service, &address(text)).await.err().unwrap().to_string();

        assert_eq!(error("/nope/d").await, "no project nope");
        assert_eq!(error("/nope/ideas/d").await, "no project nope");
        assert_eq!(error("/repo/nope/d").await, "no board /repo/nope");
        assert_eq!(error("/repo/nope").await, "no map at /repo/nope");
        assert_eq!(error("/repo/ideas/nope").await, "no map at /repo/ideas/nope");
        assert_eq!(error("/repo").await, "/repo is not a map");
    }

    #[tokio::test]
    async fn show_tells_a_project_a_board_and_a_map_apart() {
        let service = service();

        let project = target(&service, Some(&address("/repo"))).await.ok().unwrap();
        let ambiguous = target(&service, Some(&address("/repo/ideas"))).await.err().map(|e| e.to_string());
        let listed = target(&service, Some(&address("/repo/d"))).await.ok().unwrap();
        let on_board = target(&service, Some(&address("/repo/ideas/both"))).await.ok().unwrap();
        let unknown = target(&service, Some(&address("/repo/nope"))).await;

        assert!(matches!(project, Target::Project(p) if p.name.as_str() == "repo"));
        assert_eq!(ambiguous.as_deref(), Some("/repo/ideas is both a board and a map"));
        assert!(matches!(listed, Target::Map(m) if m.name.as_str() == "d"));
        assert!(matches!(on_board, Target::Map(m) if m.name.as_str() == "both"));
        assert!(unknown.is_err());
    }

    #[tokio::test]
    async fn a_board_that_is_not_a_map_name_is_listed() {
        let (repo, made) = project("repo");
        let board = BoardId::new();
        let events = vec![
            made,
            Event::board_created(Source::fixture(), Actor::Human, board, repo, N::new("plan").unwrap()),
            created(repo, "d", Places::new([Place::Board(board)]).unwrap()),
        ];
        let service = AppService::new(Log(events), NoSkills);

        let found = target(&service, Some(&address("/repo/plan"))).await.ok().unwrap();

        assert!(matches!(found, Target::Board(b) if b.board.name.as_str() == "plan" && b.maps.len() == 1));
    }

    #[tokio::test]
    async fn a_map_prints_its_list_path_else_its_first_board() {
        let service = service();

        let path = async |text| {
            let found = map(&service, &address(text)).await.ok().unwrap();
            of(&service, &found).await.unwrap()
        };

        assert_eq!(path("/repo/d").await, "/repo/d");
        assert_eq!(path("/repo/ideas/both").await, "/repo/both");
        assert_eq!(path("/repo/ideas/shelf").await, "/repo/ideas/shelf");
        assert_eq!(path("/other/d").await, "/other/d");
    }

    #[tokio::test]
    async fn the_first_board_in_places_order_names_a_map_outside_the_list() {
        let (repo, made) = project("repo");
        let (first, second) = (BoardId::new(), BoardId::new());
        let board = |id, name| Event::board_created(Source::fixture(), Actor::Human, id, repo, N::new(name).unwrap());
        let places = Places::new([Place::Board(second), Place::Board(first)]).unwrap();
        let events = vec![made, board(first, "a"), board(second, "b"), created(repo, "m", places)];
        let service = AppService::new(Log(events), NoSkills);

        let found = map(&service, &address("/repo/b/m")).await.ok().unwrap();

        assert_eq!(of(&service, &found).await.unwrap(), "/repo/a/m");
    }
}
