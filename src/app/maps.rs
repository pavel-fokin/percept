use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{AppService, Taken};
use crate::core::{Actor, Board, Event, EventId, EventStore, Map, MapId, Name, NodeId, Payload, Place, Places, Policy, Project, ProjectId, Reference, SessionId, Skill, SkillStore, Source};
use crate::shared::Result;

#[derive(Debug, PartialEq)]
pub enum Ref {
    Message(String),
    File { path: PathBuf, lines: (u32, u32) },
}

/// A map with its nodes newest first and its edges in log order.
/// `messages` holds every message its nodes cite; `boards` the boards the map is on, in creation order.
pub struct MapDetail {
    pub map: Map,
    pub boards: Vec<Board>,
    pub messages: BTreeMap<EventId, CitedMessage>,
}

/// Where a cited message sits: its session and when it was said.
#[derive(Clone, Copy)]
pub struct CitedMessage {
    pub session: SessionId,
    pub created_at: jiff::Timestamp,
}

impl<S: EventStore, M: SkillStore> AppService<S, M> {
    /// Records a new map defined by the skill directory `skill`, an absolute path, or by no skill, in the project's map list.
    /// No board or map of the project may have the name. Returns the new map.
    pub async fn create_map(
        &self,
        source: Source,
        actor: Actor,
        project: ProjectId,
        name: &str,
        skill: Option<&Path>,
    ) -> Result<Map> {
        let places = Places::new([Place::Project(project)])?;

        self.create_map_at(source, actor, project, places, name, skill).await
    }

    /// Like `create_map`, but the map is shown on the board only, not in the project's map list.
    pub async fn create_map_on_board(
        &self,
        source: Source,
        actor: Actor,
        board: &Board,
        name: &str,
        skill: Option<&Path>,
    ) -> Result<Map> {
        let places = Places::new([Place::Board(board.id)])?;

        self.create_map_at(source, actor, board.project, places, name, skill).await
    }

    async fn create_map_at(
        &self,
        source: Source,
        actor: Actor,
        project: ProjectId,
        places: Places,
        name: &str,
        skill: Option<&Path>,
    ) -> Result<Map> {
        let name = Name::new(name)?;
        if let Some(skill) = skill.filter(|s| !s.is_absolute()) {
            return Err(format!("the skill {} must be an absolute path", skill.display()).into());
        }

        let events = self.store.all().await?;
        Project::check_free(&events, project, &name).map_err(|taken| Taken(taken.to_string()))?;
        places.check_in(project, &Board::all(&events))?;

        let schema = match skill {
            Some(skill) => self.skills.skill(skill).await?.schema().cloned(),
            None => None,
        };

        let event = Event::map_created(source, actor, MapId::new(), project, name, skill.map(Into::into), schema, places);
        self.store.append(&event).await?;

        Ok(Map::all(&[event]).remove(0))
    }

    /// Records that the map is shown at `place` too. Nothing is written when it already is.
    pub async fn place_map(&self, source: Source, actor: Actor, map: MapId, place: Place) -> Result<()> {
        let Located { events, map } = self.located(map).await?;

        let places = map.places.with(place);
        if places == map.places {
            return Ok(());
        }
        places.check_in(map.project, &Board::all(&events))?;

        self.store.append(&Event::map_changed(source, actor, map.id, places)).await
    }

    /// Records that the map is no longer shown at `place`. Nothing is written when it was not.
    /// A map shown only at `place` fails with `LastPlace`.
    pub async fn unplace_map(&self, source: Source, actor: Actor, map: MapId, place: Place) -> Result<()> {
        let Located { map, .. } = self.located(map).await?;

        let places = map.places.without(place)?;
        if places == map.places {
            return Ok(());
        }

        self.store.append(&Event::map_changed(source, actor, map.id, places)).await
    }

    /// The maps of the project in log order, each with its nodes and edges, from one read of the log.
    pub async fn maps(&self, project: ProjectId) -> Result<Vec<Map>> {
        Ok(Map::at(&self.store.all().await?, Place::Project(project)))
    }

    /// Every map placed in its own project, across all projects, in log order, each with its nodes and edges, from one read of the log.
    pub async fn all_maps(&self) -> Result<Vec<Map>> {
        let mut maps = Map::all(&self.store.all().await?);
        maps.retain(|m| m.places.contains(Place::Project(m.project)));

        Ok(maps)
    }

    /// The id of the map named `map` in the project named `project`, in any case.
    pub async fn map_id(&self, project: &str, map: &str) -> Result<Option<MapId>> {
        let events = self.store.all().await?;

        let Some(project) = Project::named(&events, project) else {
            return Ok(None);
        };

        Ok(Map::named(&events, project.id, map).map(|m| m.id))
    }

    /// The map, with its nodes and edges, or an error if there is none.
    pub async fn map(&self, map: MapId) -> Result<Map> {
        Ok(self.located(map).await?.map)
    }

    /// The skill the map was created from, as it reads now; `None` for a map without one.
    pub async fn skill(&self, map: &Map) -> Result<Option<Skill>> {
        match &map.skill {
            Some(skill) => Ok(Some(self.skills.skill(skill).await?)),
            None => Ok(None),
        }
    }

    /// The map, its nodes newest first and its edges in log order, from one read of the log.
    pub async fn map_detail(&self, map: MapId) -> Result<Option<MapDetail>> {
        let events = self.store.all().await?;

        let Some(mut map) = Map::all(&events).into_iter().find(|m| m.id == map) else {
            return Ok(None);
        };
        map.nodes.reverse();

        let boards = Board::all(&events).into_iter().filter(|b| map.places.contains(Place::Board(b.id))).collect();

        let cited: BTreeSet<EventId> = map
            .nodes
            .iter()
            .flat_map(|n| &n.references)
            .filter_map(|r| match r {
                Reference::Message { event, .. } => Some(*event),
                Reference::File { .. } => None,
            })
            .collect();
        let messages = events
            .iter()
            .filter(|e| cited.contains(&e.id))
            .filter_map(|e| match &e.payload {
                Payload::Message { session, .. } => Some((e.id, CitedMessage { session: *session, created_at: e.created_at })),
                _ => None,
            })
            .collect();

        Ok(Some(MapDetail { map, boards, messages }))
    }

    /// Records a new edge on the map, between these nodes.
    #[allow(clippy::too_many_arguments, reason = "the inputs for adding an edge")]
    pub async fn add_edge(
        &self,
        source: Source,
        actor: Actor,
        map: MapId,
        from: NodeId,
        kind: &str,
        to: NodeId,
    ) -> Result<()> {
        not_blank("kind", kind)?;

        let Located { map, .. } = self.located(map).await?;

        self.write(&map, Event::edge_added(source, actor, map.id, kind, from, to)).await
    }

    /// Records the removal of the edge of this kind between these nodes on the map.
    #[allow(clippy::too_many_arguments, reason = "the inputs for removing an edge")]
    pub async fn remove_edge(
        &self,
        source: Source,
        actor: Actor,
        map: MapId,
        from: NodeId,
        kind: &str,
        to: NodeId,
    ) -> Result<()> {
        let Located { map, .. } = self.located(map).await?;

        self.write(&map, Event::edge_removed(source, actor, map.id, kind, from, to)).await
    }

    /// Records the removal of the node, and with it every edge from or to it, from the map.
    pub async fn remove_node(&self, source: Source, actor: Actor, map: MapId, node: NodeId) -> Result<()> {
        let Located { map, .. } = self.located(map).await?;

        self.write(&map, Event::node_removed(source, actor, map.id, node)).await
    }

    /// Records a new node on the map. A title is unique within its map.
    #[allow(clippy::too_many_arguments, reason = "the inputs for adding a node")]
    pub async fn add_node(
        &self,
        source: Source,
        actor: Actor,
        map: MapId,
        kind: &str,
        title: &str,
        properties: BTreeMap<String, String>,
        references: Vec<Ref>,
    ) -> Result<()> {
        not_blank("kind", kind)?;

        let Located { events, map } = self.located(map).await?;

        let mut resolved = Vec::new();
        for reference in references {
            resolved.push(resolve_reference(&events, map.project, &source, reference).await?);
        }

        self.write(&map, Event::node_added(source, actor, map.id, NodeId::new(), kind, title, properties, resolved)).await
    }

    /// Records the full new state of the node on the map.
    /// Each property in `properties` is set; the others keep their values.
    /// `references` are resolved and appended to the node's own. A new title is unique within its map.
    #[allow(clippy::too_many_arguments, reason = "the inputs for changing a node")]
    pub async fn change_node(
        &self,
        source: Source,
        actor: Actor,
        map: MapId,
        node: NodeId,
        new_title: Option<&str>,
        properties: BTreeMap<String, String>,
        references: Vec<Ref>,
    ) -> Result<()> {
        let Located { events, map } = self.located(map).await?;
        let node = map.on_map(node)?;

        let new_title = new_title.unwrap_or(&node.title);

        let mut merged = node.properties.clone();
        merged.extend(properties);

        if new_title == node.title && merged == node.properties && references.is_empty() {
            return Err(format!("the change leaves {:?} as it is", node.title).into());
        }

        let mut all_references = node.references.clone();
        for reference in references {
            all_references.push(resolve_reference(&events, map.project, &source, reference).await?);
        }

        self.write(&map, Event::node_changed(source, actor, map.id, node.id, new_title, merged, all_references)).await
    }

    /// The log and the map with this id.
    async fn located(&self, map: MapId) -> Result<Located> {
        let events = self.store.all().await?;

        let map = Map::all(&events).into_iter().find(|m| m.id == map).ok_or("no such map")?;

        Ok(Located { events, map })
    }

    /// The one way a node or an edge event reaches the log: the map, under its schema, and then the policy see it first.
    async fn write(&self, map: &Map, event: Event) -> Result<()> {
        map.check(&event)?;
        Policy::check(&event, map)?;

        self.store.append(&event).await
    }
}

/// A map found in its project, with the log it was read from.
struct Located {
    events: Vec<Event>,
    map: Map,
}

/// File references resolve against the writer's checkout and must stay inside it; the web has none.
async fn resolve_reference(events: &[Event], project: ProjectId, source: &Source, reference: Ref) -> Result<Reference> {
    match reference {
        Ref::Message(quote) => {
            not_blank("message reference", &quote)?;

            let sessions: BTreeSet<SessionId> = events
                .iter()
                .filter_map(|event| match &event.payload {
                    Payload::SessionCreated { session, project: of, .. } if *of == project => Some(*session),
                    _ => None,
                })
                .collect();

            let event = [Actor::Human, Actor::Agent].into_iter().find_map(|actor| {
                events.iter().rev().find(|event| {
                    event.actor == actor
                        && matches!(&event.payload, Payload::Message { session, content } if sessions.contains(session) && content.contains(&quote))
                })
            }).ok_or("no matching message in this project; copy the quote verbatim from a recorded message")?;

            Ok(Reference::Message { event: event.id, quote })
        }
        Ref::File { path, lines: (from, to) } => {
            if path.as_os_str().is_empty() || path.is_absolute() {
                return Err("a file reference path must be nonempty and relative to the project".into());
            }
            if from == 0 || from > to {
                return Err("a file reference range must use 1-based lines with from <= to".into());
            }

            let folder = source.path()?;
            let folder = tokio::fs::canonicalize(folder).await
                .map_err(|error| format!("cannot read {}: {error}", folder.display()))?;
            let target = tokio::fs::canonicalize(folder.join(&path)).await
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let relative = target.strip_prefix(&folder).map_err(|_| "a file reference must stay inside the project")?.to_path_buf();
            let text = tokio::fs::read_to_string(&target).await
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;

            let excerpt: Vec<_> = text.lines().skip((from - 1) as usize).take((to - from + 1) as usize).collect();
            if excerpt.len() != (to - from + 1) as usize {
                return Err(format!("file reference {}:{from}-{to} exceeds the file's lines", path.display()).into());
            }

            Ok(Reference::File { path: relative, lines: (from, to), excerpt: excerpt.join("\n") })
        }
    }
}

fn not_blank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(format!("{name} must not be blank").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BoardId, Node};
    use crate::app::testing::{Broken, Memory, NoSkills, Skills, add_project, add_session, id_at, in_project};

    #[tokio::test]
    async fn a_placed_map_is_shown_there_and_placing_again_writes_nothing() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let map = service.map_id("p0", "decisions").await.unwrap().unwrap();
        let board = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap().id;

        service.place_map(Source::Web, Actor::Human, map, Place::Board(board)).await.unwrap();
        let written = store.0.lock().unwrap().len();
        service.place_map(Source::Web, Actor::Human, map, Place::Board(board)).await.unwrap();
        service.place_map(Source::Web, Actor::Human, map, Place::Project(project)).await.unwrap();

        let places = service.map(map).await.unwrap().places;
        assert!(places.contains(Place::Project(project)) && places.contains(Place::Board(board)));
        assert_eq!(store.0.lock().unwrap().len(), written);
        assert!(matches!(store.0.lock().unwrap().last().unwrap().payload, Payload::MapChanged { .. }));
    }

    #[tokio::test]
    async fn an_unplaced_map_stays_elsewhere_and_its_last_place_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let map = service.map_id("p0", "decisions").await.unwrap().unwrap();
        let board = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap().id;
        service.place_map(Source::Web, Actor::Human, map, Place::Board(board)).await.unwrap();

        service.unplace_map(Source::Web, Actor::Human, map, Place::Board(board)).await.unwrap();
        let written = store.0.lock().unwrap().len();
        service.unplace_map(Source::Web, Actor::Human, map, Place::Board(board)).await.unwrap();
        let last = service.unplace_map(Source::Web, Actor::Human, map, Place::Project(project)).await;

        let places = service.map(map).await.unwrap().places;
        assert!(places.contains(Place::Project(project)) && !places.contains(Place::Board(board)));
        assert!(last.unwrap_err().is::<crate::core::LastPlace>());
        assert_eq!(store.0.lock().unwrap().len(), written);
    }

    #[tokio::test]
    async fn a_map_is_not_placed_in_another_project_or_on_a_board_of_one() {
        let store = in_project("/repo");
        add_project(&store, "/other");
        let (project, other) = (id_at(&store, "/repo"), id_at(&store, "/other"));
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let map = service.map_id("p0", "decisions").await.unwrap().unwrap();
        let foreign_board = service.create_board(Source::Web, Actor::Human, "p1").await.unwrap().unwrap().id;
        let written = store.0.lock().unwrap().len();

        let to_project = service.place_map(Source::Web, Actor::Human, map, Place::Project(other)).await;
        let to_foreign_board = service.place_map(Source::Web, Actor::Human, map, Place::Board(foreign_board)).await;
        let to_unknown_board = service.place_map(Source::Web, Actor::Human, map, Place::Board(BoardId::new())).await;
        let unknown_map = service.place_map(Source::Web, Actor::Human, MapId::new(), Place::Project(project)).await;

        assert!(to_project.is_err() && to_foreign_board.is_err() && to_unknown_board.is_err() && unknown_map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), written);
    }

    #[tokio::test]
    async fn the_same_map_name_can_be_created_in_two_projects_but_not_twice_in_one() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        add_project(&store, "/other");
        let other = Source::Checkout { client: "claude-code".into(), path: "/other".into() };
        let other_project = id_at(&store, "/other");
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(other, Actor::Human, other_project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();

        let again = service.create_map(Source::fixture(), Actor::Agent, project, "decisions", Some(Path::new("/skills/decisions"))).await;

        assert!(again.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 4);
    }

    #[tokio::test]
    async fn a_created_map_is_returned_with_its_places() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        let board = service.create_named_board(Source::Web, Actor::Human, project, "storage").await.unwrap();

        let listed = service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let shelved = service.create_map_on_board(Source::fixture(), Actor::Human, &board, "choice", Some(Path::new("/skills/decisions"))).await.unwrap();

        assert_eq!(listed.id, service.map_id("p0", "decisions").await.unwrap().unwrap());
        assert!(listed.places.contains(Place::Project(project)));
        assert_eq!(shelved.id, service.map_id("p0", "choice").await.unwrap().unwrap());
        assert!(shelved.places.contains(Place::Board(board.id)));
    }

    #[tokio::test]
    async fn a_map_is_refused_a_name_a_board_has() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_named_board(Source::Web, Actor::Human, project, "storage").await.unwrap();

        let result = service.create_map(Source::fixture(), Actor::Human, project, "Storage", Some(Path::new("/skills/decisions"))).await;

        assert_eq!(result.err().unwrap().to_string(), "this project already has a storage board");
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_map_created_on_a_board_is_shown_there_only() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        add_project(&store, "/other");
        let service = AppService::new(&store, Skills);
        let board = service.create_named_board(Source::Web, Actor::Human, project, "storage").await.unwrap();
        let foreign = service.create_named_board(Source::Web, Actor::Human, id_at(&store, "/other"), "far").await.unwrap();

        service.create_map_on_board(Source::fixture(), Actor::Agent, &board, "choice", Some(Path::new("/skills/decisions"))).await.unwrap();
        let taken = service.create_map_on_board(Source::fixture(), Actor::Agent, &board, "storage", Some(Path::new("/skills/decisions"))).await;
        let elsewhere = service.create_map_on_board(Source::fixture(), Actor::Agent, &Board { project, ..foreign }, "far-map", Some(Path::new("/skills/decisions"))).await;

        let map = service.map_id("p0", "choice").await.unwrap().unwrap();
        let places = service.map(map).await.unwrap().places;
        assert!(places.contains(Place::Board(board.id)) && !places.contains(Place::Project(project)));
        assert!(service.maps(project).await.unwrap().is_empty());
        assert!(taken.is_err() && elsewhere.is_err());
    }

    #[tokio::test]
    async fn a_map_name_is_validated_and_ignores_case() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);

        let invalid = service.create_map(Source::fixture(), Actor::Human, project, "my map", Some(Path::new("/skills/decisions"))).await;
        service.create_map(Source::fixture(), Actor::Human, project, "Decisions", Some(Path::new("/skills/decisions"))).await.unwrap();

        assert!(invalid.is_err());
        assert_eq!(service.maps(project).await.unwrap()[0].name.as_str(), "decisions");
    }

    #[tokio::test]
    async fn a_map_is_created_only_from_a_skill() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);

        let with_skill = service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await;
        let without_skill = service.create_map(Source::fixture(), Actor::Human, project, "nope", Some(Path::new("/skills/nope"))).await;

        assert!(with_skill.is_ok());
        assert!(without_skill.err().unwrap().to_string().contains("nope"));
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_map_is_checked_against_its_own_file_whatever_its_kind() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "choices", Some(Path::new("/elsewhere/decisions"))).await.unwrap();

        let added = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "choices").await, "question", "Q", Default::default(), Vec::new()).await;
        let map = service.map(mid(&service, project, "choices").await).await.unwrap();

        assert!(added.is_ok());
        assert_eq!(map.skill.as_deref(), Some(Path::new("/elsewhere/decisions")));
    }

    #[tokio::test]
    async fn a_map_without_a_skill_accepts_any_kind_and_reads_no_skill() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, NoSkills);
        service.create_map(Source::fixture(), Actor::Human, project, "scratch", None).await.unwrap();
        let id = mid(&service, project, "scratch").await;

        let added = service.add_node(Source::fixture(), Actor::Human, id, "anything", "A", Default::default(), Vec::new()).await;
        let map = service.map(id).await.unwrap();
        let skill = service.skill(&map).await.unwrap();

        assert!(added.is_ok());
        assert!(map.skill.is_none() && map.schema.is_none());
        assert!(skill.is_none());
    }

    #[tokio::test]
    async fn a_relative_skill_file_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);

        let result = service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("skills/decisions"))).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn maps_are_the_current_projects_in_log_order() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        add_project(&store, "/other");
        let other = Source::Checkout { client: "claude-code".into(), path: "/other".into() };
        let other_project = id_at(&store, "/other");
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(other, Actor::Human, other_project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();

        let maps = service.maps(project).await.unwrap();

        let found: Vec<&str> = maps.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(found, vec!["decisions", "tasks"]);
    }

    #[tokio::test]
    async fn add_node_to_a_missing_map_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();

        let result = service.add_node(Source::fixture(), Actor::Human, MapId::new(), "task", "T", Default::default(), Vec::new()).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn add_node_checks_its_kind_and_properties_against_the_schema() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let undeclared = BTreeMap::from([("colour".to_string(), "red".to_string())]);

        let unknown_kind = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "idea", "A", Default::default(), Vec::new()).await;
        let unknown_property = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "B", undeclared, Vec::new()).await;
        let subset = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "C", Default::default(), Vec::new()).await;

        assert!(unknown_kind.unwrap_err().to_string().contains("concept"));
        assert!(unknown_property.unwrap_err().to_string().contains("definition"));
        assert!(subset.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn add_edge_checks_its_kind_and_ends_against_the_schema() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();

        let unknown_kind = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "blocks", nid(&service, project, "decisions", "Q").await).await;
        let wrong_from = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await, "answers", nid(&service, project, "decisions", "Q").await).await;
        let wrong_to = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "O").await).await;
        let valid = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await;

        assert!(unknown_kind.unwrap_err().to_string().contains("answers"));
        assert!(wrong_from.unwrap_err().to_string().contains("from"));
        assert!(wrong_to.unwrap_err().to_string().contains("to"));
        assert!(valid.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn a_map_without_a_schema_takes_any_kind_and_property_but_not_a_broken_graph() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "scratch", Some(Path::new("/skills/scratch"))).await.unwrap();
        let map = mid(&service, project, "scratch").await;
        let colour = BTreeMap::from([("colour".to_string(), "red".to_string())]);
        service.add_node(Source::fixture(), Actor::Human, map, "idea", "A", colour, Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, map, "whim", "B", Default::default(), Vec::new()).await.unwrap();
        let (a, b) = (nid(&service, project, "scratch", "A").await, nid(&service, project, "scratch", "B").await);

        let edge = service.add_edge(Source::fixture(), Actor::Human, map, a, "echoes", b).await;
        let duplicate_edge = service.add_edge(Source::fixture(), Actor::Human, map, a, "echoes", b).await;
        let duplicate_title = service.add_node(Source::fixture(), Actor::Human, map, "other", "A", Default::default(), Vec::new()).await;

        assert!(edge.is_ok());
        assert!(duplicate_edge.is_err());
        assert!(duplicate_title.is_err());
    }

    #[tokio::test]
    async fn a_map_keeps_its_schema_once_its_skill_is_gone() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        let map = service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        let gone = AppService::new(&store, NoSkills);

        let valid = gone.add_node(Source::fixture(), Actor::Human, map.id, "task", "T", Default::default(), Vec::new()).await;
        let invalid = gone.add_node(Source::fixture(), Actor::Human, map.id, "bogus", "U", Default::default(), Vec::new()).await;

        assert!(valid.is_ok());
        assert!(invalid.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn add_node_records_its_properties() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let properties = BTreeMap::from([("definition".to_string(), "One log.".to_string())]);

        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Event log", properties.clone(), Vec::new()).await.unwrap();

        let nodes = service.map(mid(&service, project, "concepts").await).await.unwrap().nodes;
        assert_eq!(nodes[0].properties, properties);
    }

    #[tokio::test]
    async fn message_references_prefer_the_latest_human_in_the_project() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        add_project(&store, "/other");
        let (first, second, foreign_session) = (add_session(&store, "/repo"), add_session(&store, "/repo"), add_session(&store, "/other"));
        let human = Event::message(Source::fixture(), first, Actor::Human, "Keep the text: verbatim.", json!({}));
        let events = [
            Event::message(Source::fixture(), first, Actor::Human, "Keep the text: verbatim.", json!({})),
            human.clone(),
            Event::message(Source::fixture(), second, Actor::Agent, "Keep the text: verbatim.", json!({})),
            Event::message(Source::Checkout { client: "claude-code".into(), path: "/other".into() }, foreign_session, Actor::Human, "Keep the text: verbatim.", json!({})),
            Event::message(Source::fixture(), second, Actor::System, "Keep the text: verbatim.", json!({})),
        ];
        store.0.lock().unwrap().extend(events);

        service.add_node(
            Source::fixture(), Actor::Agent, mid(&service, project, "concepts").await, "concept", "Snapshots", BTreeMap::new(),
            vec![Ref::Message("text: verbatim".into()), Ref::Message("Keep the text".into()), Ref::Message("text: verbatim".into())],
        ).await.unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 12);
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![
            Reference::Message { event: human.id, quote: "text: verbatim".into() },
            Reference::Message { event: human.id, quote: "Keep the text".into() },
            Reference::Message { event: human.id, quote: "text: verbatim".into() },
        ]);
    }

    #[tokio::test]
    async fn a_message_reference_finds_another_session_of_the_project_but_not_of_another() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        add_project(&store, "/other");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let (own, foreign) = (add_session(&store, "/repo/src"), add_session(&store, "/other"));
        let said = Event::message(Source::fixture(), own, Actor::Human, "Only here", json!({}));
        let elsewhere = Event::message(Source::Checkout { client: "claude-code".into(), path: "/other".into() }, foreign, Actor::Human, "Only there", json!({}));
        store.0.lock().unwrap().extend([said.clone(), elsewhere]);

        let found = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "A", BTreeMap::new(), vec![Ref::Message("Only here".into())]).await;
        let missing = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "B", BTreeMap::new(), vec![Ref::Message("Only there".into())]).await;

        found.unwrap();
        assert!(missing.unwrap_err().to_string().contains("copy the quote verbatim"));
        let events = store.0.lock().unwrap();
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![Reference::Message { event: said.id, quote: "Only here".into() }]);
    }

    #[tokio::test]
    async fn message_references_fall_back_to_the_latest_agent() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let session = add_session(&store, "/repo");
        let latest = Event::message(Source::fixture(), session, Actor::Agent, "A --flag: exact text Z", json!({}));
        store.0.lock().unwrap().extend([
            Event::message(Source::fixture(), session, Actor::Agent, "A --flag: exact text Z", json!({})),
            latest.clone(),
            Event::message(Source::fixture(), session, Actor::Human, "Unrelated words", json!({})),
        ]);

        service.add_node(
            Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Flags", BTreeMap::new(),
            vec![Ref::Message(" --flag: exact text ".into())],
        ).await.unwrap();

        let events = store.0.lock().unwrap();
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![Reference::Message { event: latest.id, quote: " --flag: exact text ".into() }]);
    }

    #[tokio::test]
    async fn a_bad_message_reference_writes_no_node_or_partial_references() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let session = add_session(&store, "/repo");
        store.0.lock().unwrap().push(Event::message(
            Source::fixture(), session, Actor::Human, "Exact text", json!({}),
        ));

        for quote in ["", " \n ", "exact text", "Exact  text", "missing"] {
            let result = service.add_node(
                Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Snapshots", BTreeMap::new(),
                vec![Ref::Message("Exact text".into()), Ref::Message(quote.into())],
            ).await;

            let error = result.unwrap_err().to_string();
            assert!(error.contains(if quote.trim().is_empty() { "blank" } else { "copy the quote verbatim" }));
            assert_eq!(store.0.lock().unwrap().len(), 4);
        }
    }

    #[tokio::test]
    async fn system_messages_and_non_messages_are_not_reference_sources() {
        use serde_json::json;

        let store = in_project("/repo");
        let session = add_session(&store, "/repo");
        let events = [
            Event::message(Source::fixture(), session, Actor::System, "Match", json!({})),
            Event::subagent_reported(Source::fixture(), session, crate::core::ClientAgentId::new("a".into()), "Match", json!({})),
        ];
        store.0.lock().unwrap().extend(events);
        let events = store.0.lock().unwrap().clone();
        let project = Project::all(&events).remove(0);

        let result = resolve_reference(&events, project.id, &Source::Web, Ref::Message("Match".into())).await;

        assert!(result.is_err());
    }

    async fn file_project() -> Source {
        let path = std::env::temp_dir().join(format!("percept-references-{}", uuid::Uuid::now_v7()));
        tokio::fs::create_dir_all(&path).await.unwrap();

        Source::Checkout { client: "percept-cli".into(), path }
    }

    #[tokio::test]
    async fn a_web_source_cannot_write_a_file_reference() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();

        let result = service.add_node(
            Source::Web, Actor::Human, mid(&service, project, "concepts").await, "concept", "Excerpts", BTreeMap::new(),
            vec![Ref::File { path: "text.txt".into(), lines: (1, 1) }],
        ).await;

        assert!(result.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn file_references_save_inclusive_single_and_last_lines() {
        let source = file_project().await;
        tokio::fs::write(source.path().unwrap().join("text.txt"), "first\r\n\nthird\nlast").await.unwrap();
        let store = in_project(source.path().unwrap().to_str().unwrap());
        let project = id_at(&store, source.path().unwrap().to_str().unwrap());
        let service = AppService::new(&store, Skills);
        service.create_map(source.clone(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();

        service.add_node(
            source.clone(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Excerpts", BTreeMap::new(),
            vec![
                Ref::File { path: "text.txt".into(), lines: (1, 1) },
                Ref::File { path: "text.txt".into(), lines: (2, 3) },
                Ref::File { path: "text.txt".into(), lines: (4, 4) },
            ],
        ).await.unwrap();
        tokio::fs::write(source.path().unwrap().join("text.txt"), "changed").await.unwrap();

        let nodes = service.map(mid(&service, project, "concepts").await).await.unwrap().nodes;
        assert_eq!(nodes[0].references, vec![
            Reference::File { path: "text.txt".into(), lines: (1, 1), excerpt: "first".into() },
            Reference::File { path: "text.txt".into(), lines: (2, 3), excerpt: "\nthird".into() },
            Reference::File { path: "text.txt".into(), lines: (4, 4), excerpt: "last".into() },
        ]);
        assert_eq!(store.0.lock().unwrap().len(), 3);
        tokio::fs::remove_dir_all(source.path().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn mixed_references_keep_order_multiplicity_and_canonical_paths() {
        use serde_json::json;

        let source = file_project().await;
        tokio::fs::create_dir(source.path().unwrap().join("src")).await.unwrap();
        tokio::fs::write(source.path().unwrap().join("src/a:b.rs"), "code\n").await.unwrap();
        let store = in_project(source.path().unwrap().to_str().unwrap());
        let project = id_at(&store, source.path().unwrap().to_str().unwrap());
        let service = AppService::new(&store, Skills);
        service.create_map(source.clone(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let session = add_session(&store, source.path().unwrap().to_str().unwrap());
        let message = Event::message(source.clone(), session, Actor::Human, "Keep code", json!({}));
        store.0.lock().unwrap().push(message.clone());

        service.add_node(
            source.clone(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Code", BTreeMap::new(),
            vec![
                Ref::File { path: "./src/../src/a:b.rs".into(), lines: (1, 1) },
                Ref::Message("Keep code".into()),
                Ref::File { path: "src/a:b.rs".into(), lines: (1, 1) },
            ],
        ).await.unwrap();

        let events = store.0.lock().unwrap().clone();
        let Payload::NodeAdded { references, .. } = &events.last().unwrap().payload else { panic!("expected node.added") };
        assert_eq!(references, &vec![
            Reference::File { path: "src/a:b.rs".into(), lines: (1, 1), excerpt: "code".into() },
            Reference::Message { event: message.id, quote: "Keep code".into() },
            Reference::File { path: "src/a:b.rs".into(), lines: (1, 1), excerpt: "code".into() },
        ]);
        assert_eq!(events.len(), 5);
        tokio::fs::remove_dir_all(source.path().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn invalid_file_references_leave_the_log_unchanged() {
        let source = file_project().await;
        tokio::fs::write(source.path().unwrap().join("text.txt"), "first\nsecond\n").await.unwrap();
        tokio::fs::write(source.path().unwrap().join("empty.txt"), "").await.unwrap();
        tokio::fs::write(source.path().unwrap().join("binary"), [0xff]).await.unwrap();
        let store = in_project(source.path().unwrap().to_str().unwrap());
        let project = id_at(&store, source.path().unwrap().to_str().unwrap());
        let service = AppService::new(&store, Skills);
        service.create_map(source.clone(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let invalid = [
            Ref::File { path: "".into(), lines: (1, 1) },
            Ref::File { path: source.path().unwrap().join("text.txt"), lines: (1, 1) },
            Ref::File { path: "missing".into(), lines: (1, 1) },
            Ref::File { path: ".".into(), lines: (1, 1) },
            Ref::File { path: "empty.txt".into(), lines: (1, 1) },
            Ref::File { path: "binary".into(), lines: (1, 1) },
            Ref::File { path: "text.txt".into(), lines: (0, 1) },
            Ref::File { path: "text.txt".into(), lines: (2, 1) },
            Ref::File { path: "text.txt".into(), lines: (1, 3) },
            Ref::File { path: "text.txt".into(), lines: (3, 3) },
        ];

        for reference in invalid {
            let result = service.add_node(
                source.clone(), Actor::Human, mid(&service, project, "concepts").await, "concept", "Excerpts", BTreeMap::new(),
                vec![Ref::File { path: "text.txt".into(), lines: (1, 1) }, reference],
            ).await;

            assert!(result.is_err());
            assert_eq!(store.0.lock().unwrap().len(), 2);
        }
        tokio::fs::remove_dir_all(source.path().unwrap()).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn file_references_resolve_internal_symlinks_and_reject_escapes() {
        let source = file_project().await;
        let project = source.path().unwrap().join("project");
        tokio::fs::create_dir(&project).await.unwrap();
        tokio::fs::write(project.join("inside.txt"), "inside").await.unwrap();
        tokio::fs::write(source.path().unwrap().join("outside.txt"), "outside").await.unwrap();
        tokio::fs::symlink("inside.txt", project.join("alias.txt")).await.unwrap();
        tokio::fs::symlink("../outside.txt", project.join("escape.txt")).await.unwrap();

        let stub = Project { id: crate::core::ProjectId::new(), name: crate::core::Name::new("stub").unwrap(), root: Some(project.clone()) };

        let writer = Source::Checkout { client: "claude-code".into(), path: project.clone() };

        let inside = resolve_reference(&[], stub.id, &writer, Ref::File { path: "alias.txt".into(), lines: (1, 1) }).await.unwrap();
        let escape = resolve_reference(&[], stub.id, &writer, Ref::File { path: "escape.txt".into(), lines: (1, 1) }).await;
        let parent = resolve_reference(&[], stub.id, &writer, Ref::File { path: "../outside.txt".into(), lines: (1, 1) }).await;

        assert_eq!(inside, Reference::File { path: "inside.txt".into(), lines: (1, 1), excerpt: "inside".into() });
        assert!(escape.is_err());
        assert!(parent.is_err());
        tokio::fs::remove_dir_all(source.path().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn a_file_reference_resolves_against_the_writers_folder_not_the_project_root() {
        let source = file_project().await;
        let worktree = source.path().unwrap().join("worktree");
        tokio::fs::create_dir(&worktree).await.unwrap();
        tokio::fs::write(worktree.join("only-here.txt"), "mine").await.unwrap();
        let store = in_project(source.path().unwrap().to_str().unwrap());
        let project = id_at(&store, source.path().unwrap().to_str().unwrap());
        let service = AppService::new(&store, Skills);
        service.create_map(source.clone(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let writer = Source::Checkout { client: "claude-code".into(), path: worktree };

        let found = service.add_node(
            writer.clone(), Actor::Agent, mid(&service, project, "concepts").await, "concept", "A", BTreeMap::new(),
            vec![Ref::File { path: "only-here.txt".into(), lines: (1, 1) }],
        ).await;
        let at_root = service.add_node(
            source.clone(), Actor::Agent, mid(&service, project, "concepts").await, "concept", "B", BTreeMap::new(),
            vec![Ref::File { path: "only-here.txt".into(), lines: (1, 1) }],
        ).await;

        found.unwrap();
        assert!(at_root.is_err());
        tokio::fs::remove_dir_all(source.path().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn blank_kinds_and_titles_are_errors() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();

        let map = service.create_map(Source::fixture(), Actor::Human, project, " ", Some(Path::new("/skills/blank"))).await;
        let node = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "", Default::default(), Vec::new()).await;
        let edge = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await, "", nid(&service, project, "decisions", "Q").await).await;

        assert!(map.is_err());
        assert!(node.is_err());
        assert!(edge.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_repeated_edge_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let again = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await;
        let other_kind = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await, "raises", nid(&service, project, "decisions", "O").await).await;

        assert!(again.is_err());
        assert!(other_kind.is_ok());
    }

    #[tokio::test]
    async fn a_node_title_is_unique_within_its_map_but_not_across_maps() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Why?", Default::default(), Vec::new()).await.unwrap();

        let same_map = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "Why?", Default::default(), Vec::new()).await;
        let other_map = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "tasks").await, "task", "Why?", Default::default(), Vec::new()).await;

        assert!(same_map.is_err());
        assert!(other_map.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn nodes_are_the_maps_own_in_log_order() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "tasks").await, "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();

        let map = service.map(mid(&service, project, "decisions").await).await.unwrap();

        let found: Vec<(&str, &str)> = map.nodes.iter().map(|n| (n.kind.as_str(), n.title.as_str())).collect();
        assert_eq!(found, vec![("question", "Q"), ("option", "O")]);
        assert!(service.map(MapId::new()).await.is_err());
    }

    #[tokio::test]
    async fn an_edge_links_two_titled_nodes_and_a_missing_title_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();

        let missing_to = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", NodeId::new()).await;
        let missing_from = service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, NodeId::new(), "answers", nid(&service, project, "decisions", "Q").await).await;
        let missing_map = service.add_edge(Source::fixture(), Actor::Human, MapId::new(), NodeId::new(), "answers", NodeId::new()).await;
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        assert!(missing_to.unwrap_err().to_string().contains("no such node"));
        assert!(missing_from.is_err());
        assert!(missing_map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn edges_are_the_maps_own_in_log_order_and_name_node_ids() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        for (map, kind, title) in [("decisions", "question", "Q"), ("decisions", "option", "O"), ("tasks", "task", "T"), ("tasks", "task", "U")] {
            service.add_node(Source::fixture(), Actor::Human, mid(&service, project, map).await, kind, title, Default::default(), Vec::new()).await.unwrap();
        }
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "tasks").await, nid(&service, project, "tasks", "T").await, "blocks", nid(&service, project, "tasks", "U").await).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await, "raises", nid(&service, project, "decisions", "O").await).await.unwrap();

        let map = service.map(mid(&service, project, "decisions").await).await.unwrap();

        let found: Vec<(&str, NodeId, NodeId)> = map.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        let nodes = &map.nodes;
        assert_eq!(found, vec![("answers", nodes[1].id, nodes[0].id), ("raises", nodes[0].id, nodes[1].id)]);
    }

    #[tokio::test]
    async fn maps_carry_each_maps_nodes_and_edges() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "tasks").await, "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let maps = service.maps(project).await.unwrap();

        let found: Vec<(&str, usize, usize)> = maps.iter().map(|m| (m.name.as_str(), m.nodes.len(), m.edges.len())).collect();
        assert_eq!(found, vec![("decisions", 2, 1), ("tasks", 1, 0)]);
    }

    #[tokio::test]
    async fn all_maps_span_projects() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        add_project(&store, "/other");
        let service = AppService::new(&store, Skills);
        let other = Source::Checkout { client: "claude-code".into(), path: "/other".into() };
        let other_project = id_at(&store, "/other");
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(other.clone(), Actor::Human, other_project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.add_node(other, Actor::Human, mid(&service, other_project, "tasks").await, "task", "T", Default::default(), Vec::new()).await.unwrap();

        let maps = service.all_maps().await.unwrap();

        let found: Vec<(&str, usize)> = maps.iter().map(|m| (m.name.as_str(), m.nodes.len())).collect();
        assert_eq!(found, vec![("decisions", 0), ("tasks", 1)]);
        assert_ne!(maps[0].project, maps[1].project);
    }

    #[tokio::test]
    async fn listings_leave_out_a_map_placed_only_on_a_board() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let places = Places::new([Place::Board(crate::core::BoardId::new())]).unwrap();
        let draft = Event::map_created(Source::fixture(), Actor::Human, MapId::new(), project, Name::new("draft").unwrap(), Some("/skills/draft".into()), None, places);
        store.0.lock().unwrap().push(draft);

        let in_project = service.maps(project).await.unwrap();
        let all = service.all_maps().await.unwrap();

        assert_eq!(in_project.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["decisions"]);
        assert_eq!(all.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), vec!["decisions"]);
    }

    #[tokio::test]
    async fn map_detail_names_the_boards_the_map_is_on_in_creation_order() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        let map = mid(&service, project, "decisions").await;
        let first = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap().id;
        let second = service.create_board(Source::Web, Actor::Human, "p0").await.unwrap().unwrap().id;
        service.create_board(Source::Web, Actor::Human, "p0").await.unwrap();
        let none = service.map_detail(map).await.unwrap().unwrap().boards;
        service.place_map(Source::Web, Actor::Human, map, Place::Board(second)).await.unwrap();
        service.place_map(Source::Web, Actor::Human, map, Place::Board(first)).await.unwrap();

        let boards = service.map_detail(map).await.unwrap().unwrap().boards;

        assert!(none.is_empty());
        assert_eq!(boards.iter().map(|b| b.id).collect::<Vec<_>>(), vec![first, second]);
    }

    #[tokio::test]
    async fn map_detail_has_nodes_newest_first_and_edges_in_log_order() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.create_map(Source::fixture(), Actor::Human, project, "tasks", Some(Path::new("/skills/tasks"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "tasks").await, "task", "T", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let detail = service.map_detail(mid(&service, project, "decisions").await).await.unwrap().unwrap();

        let titles: Vec<&str> = detail.map.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["O", "Q"]);
        let edges: Vec<(&str, NodeId, NodeId)> = detail.map.edges.iter().map(|e| (e.kind.as_str(), e.from, e.to)).collect();
        assert_eq!(edges, vec![("answers", detail.map.nodes[0].id, detail.map.nodes[1].id)]);
        assert_eq!(detail.map.name.as_str(), "decisions");
    }

    #[tokio::test]
    async fn map_detail_locates_the_messages_its_nodes_cite() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "concepts", Some(Path::new("/skills/concepts"))).await.unwrap();
        let session = add_session(&store, "/repo");
        let cited = Event::message(Source::fixture(), session, Actor::Human, "Keep the text", json!({}));
        let other = Event::message(Source::fixture(), session, Actor::Human, "Unrelated", json!({}));
        store.0.lock().unwrap().extend([cited.clone(), other]);
        service.add_node(
            Source::fixture(), Actor::Human, mid(&service, project, "concepts").await, "concept", "C", BTreeMap::new(),
            vec![Ref::Message("Keep the text".into())],
        ).await.unwrap();

        let detail = service.map_detail(mid(&service, project, "concepts").await).await.unwrap().unwrap();

        assert_eq!(detail.messages.len(), 1);
        assert_eq!(detail.messages[&cited.id].session, session);
        assert_eq!(detail.messages[&cited.id].created_at, cited.created_at);
    }

    #[tokio::test]
    async fn an_unknown_map_has_no_id_and_no_detail() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();

        let no_project = service.map_id("nope", "decisions").await.unwrap();
        let no_map = service.map_id("p0", "tasks").await.unwrap();
        let any_case = service.map_id("P0", "DECISIONS").await.unwrap();
        let no_detail = service.map_detail(MapId::new()).await.unwrap();

        assert!(no_project.is_none());
        assert!(no_map.is_none());
        assert_eq!(any_case, Some(mid(&service, project, "decisions").await));
        assert!(no_detail.is_none());
    }

    async fn mid<S: EventStore, M: SkillStore>(service: &AppService<S, M>, project: ProjectId, name: &str) -> MapId {
        Map::named(&service.store.all().await.unwrap(), project, name).unwrap().id
    }

    async fn nid<S: EventStore, M: SkillStore>(service: &AppService<S, M>, project: ProjectId, map: &str, title: &str) -> NodeId {
        service.map(mid(service, project, map).await).await.unwrap().nodes.iter().find(|n| n.title == title).unwrap().id
    }

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    async fn glossary(store: &Memory) -> AppService<&Memory, Skills> {
        let service = AppService::new(store, Skills);
        let project = id_at(store, "/repo");
        service.create_map(Source::fixture(), Actor::Human, project, "glossary", Some(Path::new("/skills/glossary"))).await.unwrap();
        service.add_node(
            Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, "term", "Log",
            props(&[("definition", "One log."), ("source", "docs")]), Vec::new(),
        ).await.unwrap();

        service
    }

    async fn glossary_nodes(service: &AppService<&Memory, Skills>) -> Vec<Node> {
        let project = service.project_at(Path::new("/repo")).await.unwrap().unwrap().id;

        service.map(mid(service, project, "glossary").await).await.unwrap().nodes
    }

    #[tokio::test]
    async fn a_change_writes_the_full_new_state_and_keeps_unnamed_properties() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;
        let map = service.map(mid(&service, project, "glossary").await).await.unwrap();
        let before = glossary_nodes(&service).await;

        service.change_node(
            Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, Some("Event log"),
            props(&[("definition", "Append-only.")]), Vec::new(),
        ).await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.actor, Actor::Human);
        assert_eq!(last.payload, Payload::NodeChanged {
            map: map.id,
            node: before[0].id,
            title: "Event log".into(),
            properties: props(&[("definition", "Append-only."), ("source", "docs")]),
            references: Vec::new(),
        });
        let after = glossary_nodes(&service).await;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].title, "Event log");
        assert_eq!(after[0].changes.len(), 2);
    }

    #[tokio::test]
    async fn two_positionals_keep_the_title() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "spec")]), Vec::new()).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].title, "Log");
        assert_eq!(nodes[0].properties, props(&[("definition", "One log."), ("source", "spec")]));
    }

    #[tokio::test]
    async fn an_empty_value_is_set_like_any_other() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "")]), Vec::new()).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].properties, props(&[("definition", "One log."), ("source", "")]));
    }

    #[tokio::test]
    async fn a_reference_is_appended_to_the_existing_ones() {
        use serde_json::json;

        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;
        let session = add_session(&store, "/repo");
        let first = Event::message(Source::fixture(), session, Actor::Human, "First quote", json!({}));
        let second = Event::message(Source::fixture(), session, Actor::Human, "Second quote", json!({}));
        store.0.lock().unwrap().extend([first.clone(), second.clone()]);

        service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, BTreeMap::new(), vec![Ref::Message("First quote".into())]).await.unwrap();
        service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, BTreeMap::new(), vec![Ref::Message("Second quote".into())]).await.unwrap();

        let nodes = glossary_nodes(&service).await;
        assert_eq!(nodes[0].references, vec![
            Reference::Message { event: first.id, quote: "First quote".into() },
            Reference::Message { event: second.id, quote: "Second quote".into() },
        ]);
    }

    #[tokio::test]
    async fn a_rename_is_not_blank_and_unique_within_its_map_but_may_keep_the_old_title() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, "term", "Other", BTreeMap::new(), Vec::new()).await.unwrap();

        let taken = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, Some("Other"), BTreeMap::new(), Vec::new()).await;
        let blank = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, Some(" "), BTreeMap::new(), Vec::new()).await;
        let same = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, Some("Log"), props(&[("source", "spec")]), Vec::new()).await;

        assert!(taken.unwrap_err().to_string().contains("\"Other\""));
        assert!(blank.unwrap_err().to_string().contains("blank"));
        assert!(same.is_ok());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn the_merged_properties_are_checked_against_the_schema() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        let result = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("colour", "red")]), Vec::new()).await;

        assert!(result.unwrap_err().to_string().contains("colour"));
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn a_change_that_leaves_the_node_as_it_is_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        let nothing = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, BTreeMap::new(), Vec::new()).await;
        let same_title = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, Some("Log"), BTreeMap::new(), Vec::new()).await;
        let same_value = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "docs")]), Vec::new()).await;
        let absent_drop = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("definition", "One log."), ("source", "docs")]), Vec::new()).await;

        assert!(nothing.is_err());
        assert!(same_title.is_err());
        assert!(same_value.is_err());
        assert!(absent_drop.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn changing_a_missing_node_or_map_is_an_error() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        let node = service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, NodeId::new(), None, props(&[("source", "x")]), Vec::new()).await;
        let map = service.change_node(Source::fixture(), Actor::Human, MapId::new(), NodeId::new(), None, props(&[("source", "x")]), Vec::new()).await;

        assert!(node.unwrap_err().to_string().contains("no such node"));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_changing_a_human_node_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = glossary(&store).await;

        let result = service.change_node(Source::fixture(), Actor::Agent, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "x")]), Vec::new()).await;

        assert!(result.unwrap_err().to_string().contains("only a human may change it"));
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn an_agent_may_change_its_own_node_until_a_human_changes_it() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "glossary", Some(Path::new("/skills/glossary"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "glossary").await, "term", "Log", BTreeMap::new(), Vec::new()).await.unwrap();

        let agent = service.change_node(Source::fixture(), Actor::Agent, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "a")]), Vec::new()).await;
        service.change_node(Source::fixture(), Actor::Human, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "h")]), Vec::new()).await.unwrap();
        let after_human = service.change_node(Source::fixture(), Actor::Agent, mid(&service, project, "glossary").await, nid(&service, project, "glossary", "Log").await, None, props(&[("source", "b")]), Vec::new()).await;

        assert!(agent.is_ok());
        assert!(after_human.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    async fn decisions(store: &Memory, actor: Actor) -> AppService<&Memory, Skills> {
        let service = AppService::new(store, Skills);
        let project = id_at(store, "/repo");
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), actor, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        service
    }

    #[tokio::test]
    async fn removing_an_edge_drops_it_from_the_edges_and_the_summary() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;

        service.remove_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.actor, Actor::Human);
        assert_eq!(last.payload.kind(), crate::core::Kind::EdgeRemoved);
        let after = service.map(mid(&service, project, "decisions").await).await.unwrap();
        assert_eq!((after.nodes.len(), after.edges.len()), (2, 0));
    }

    #[tokio::test]
    async fn removing_a_missing_node_or_edge_is_an_error_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;

        let from = service.remove_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, NodeId::new(), "answers", nid(&service, project, "decisions", "Q").await).await;
        let to = service.remove_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", NodeId::new()).await;
        let reversed = service.remove_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await, "answers", nid(&service, project, "decisions", "O").await).await;
        let kind = service.remove_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "raises", nid(&service, project, "decisions", "Q").await).await;
        let map = service.remove_edge(Source::fixture(), Actor::Human, MapId::new(), NodeId::new(), "answers", NodeId::new()).await;

        assert!(from.unwrap_err().to_string().contains("answers"));
        assert!(to.unwrap_err().to_string().contains("answers"));
        assert!(reversed.unwrap_err().to_string().contains("answers"));
        assert!(kind.unwrap_err().to_string().contains("raises"));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_a_human_edge_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;

        let result = service.remove_edge(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"O\" answers \"Q\""));
        assert!(error.contains("only a human may remove it"));
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn an_agent_may_remove_its_own_edge_and_a_removed_edge_can_be_added_again() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Agent).await;

        let removed = service.remove_edge(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await;
        let again = service.add_edge(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await;

        assert!(removed.is_ok());
        assert!(again.is_ok());
        assert_eq!(service.map(mid(&service, project, "decisions").await).await.unwrap().edges.len(), 1);
        assert_eq!(store.0.lock().unwrap().len(), 7);
    }

    #[tokio::test]
    async fn removing_a_node_drops_it_its_edges_and_the_summary_counts() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;

        service.remove_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await).await.unwrap();

        let last = store.0.lock().unwrap().last().unwrap().clone();
        assert_eq!(last.payload.kind(), crate::core::Kind::NodeRemoved);
        let detail = service.map_detail(mid(&service, project, "decisions").await).await.unwrap().unwrap();
        let titles: Vec<&str> = detail.map.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["O"]);
        assert!(detail.map.edges.is_empty());
        let maps = service.maps(project).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (1, 0));
    }

    #[tokio::test]
    async fn a_removed_nodes_title_can_be_added_again() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;
        service.remove_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await).await.unwrap();

        let again = service.add_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await;

        assert!(again.is_ok());
        let maps = service.maps(project).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (2, 0));
    }

    #[tokio::test]
    async fn removing_a_missing_node_or_map_is_an_error_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Human).await;

        let node = service.remove_node(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, NodeId::new()).await;
        let map = service.remove_node(Source::fixture(), Actor::Human, MapId::new(), NodeId::new()).await;

        assert!(node.unwrap_err().to_string().contains("no such node"));
        assert!(map.is_err());
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_a_human_node_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = decisions(&store, Actor::Agent).await;

        let result = service.remove_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await).await;

        assert!(result.unwrap_err().to_string().contains("\"Q\" was written by a human, so only a human may remove it"));
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn the_policy_refuses_an_agent_removing_its_node_under_a_human_edge_and_the_log_stays() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Human, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let result = service.remove_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await).await;

        let error = result.unwrap_err().to_string();
        assert!(error.contains("\"O\" answers \"Q\""));
        assert!(error.contains("which removes it"));
        assert_eq!(store.0.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn an_agent_may_remove_its_node_with_agents_edges() {
        let store = in_project("/repo");
        let project = id_at(&store, "/repo");
        let service = AppService::new(&store, Skills);
        service.create_map(Source::fixture(), Actor::Human, project, "decisions", Some(Path::new("/skills/decisions"))).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, "question", "Q", Default::default(), Vec::new()).await.unwrap();
        service.add_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, "option", "O", Default::default(), Vec::new()).await.unwrap();
        service.add_edge(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "O").await, "answers", nid(&service, project, "decisions", "Q").await).await.unwrap();

        let result = service.remove_node(Source::fixture(), Actor::Agent, mid(&service, project, "decisions").await, nid(&service, project, "decisions", "Q").await).await;

        assert!(result.is_ok());
        let maps = service.maps(project).await.unwrap();
        assert_eq!((maps[0].nodes.len(), maps[0].edges.len()), (1, 0));
    }

    #[tokio::test]
    async fn create_map_passes_store_errors_through() {
        let result = AppService::new(Broken, Skills).create_map(Source::fixture(), Actor::Human, ProjectId::new(), "decisions", Some(Path::new("/skills/decisions"))).await;

        assert!(result.is_err());
    }

}
