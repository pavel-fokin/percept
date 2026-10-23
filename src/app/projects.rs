use std::path::{Path, PathBuf};

use super::AppService;
use crate::core::{Actor, Event, EventStore, Name, Project, ProjectId, SkillStore, Source};
use crate::shared::Result;

/// A name or root the log already holds, for a project, map or board; the message says which.
#[derive(Debug)]
pub struct Taken(pub(super) String);

impl std::fmt::Display for Taken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Taken {}

impl<S: EventStore, M: SkillStore> AppService<S, M> {
    /// Records a new project named `name`, rooted at `root` when it has one.
    /// A root holds one project, and a name belongs to one; a clash fails with `Taken`.
    pub async fn create_project(
        &self,
        source: Source,
        actor: Actor,
        name: Name,
        root: Option<PathBuf>,
    ) -> Result<Project> {
        let projects = Project::all(&self.store.all().await?);
        if let Some(root) = &root
            && projects.iter().any(|p| p.root.as_ref() == Some(root))
        {
            return Err(Taken(format!("{} is already a project", root.display())).into());
        }
        if projects.iter().any(|p| p.name == name) {
            return Err(Taken(format!("the name {} is taken", name.as_str())).into());
        }

        let project = Project { id: ProjectId::new(), name, root };
        self.store.append(&Event::project_created(source, actor, project.id, project.name.clone(), project.root.clone())).await?;

        Ok(project)
    }

    /// Every project in `ProjectCreated` order.
    pub async fn projects(&self) -> Result<Vec<Project>> {
        Ok(Project::all(&self.store.all().await?))
    }

    /// The project named `name`, in any case, if any.
    pub async fn project_named(&self, name: &str) -> Result<Option<Project>> {
        Ok(Project::named(&self.store.all().await?, name))
    }

    /// The nearest project at or above `path`, if any.
    pub async fn project_at(&self, path: &Path) -> Result<Option<Project>> {
        Ok(Project::containing(&self.store.all().await?, path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Memory, Skills};

    fn name(name: &str) -> Name {
        Name::new(name).unwrap()
    }

    fn root(path: &str) -> Option<PathBuf> {
        Some(path.into())
    }

    fn source(path: &str) -> Source {
        Source::Checkout { client: "percept-cli".into(), path: path.into() }
    }

    #[tokio::test]
    async fn init_records_the_project_under_the_given_name() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);

        let named = service.create_project(source("/work/a"), Actor::Human, name("Alpha"), root("/work/a")).await.unwrap();

        assert_eq!(named.name.as_str(), "alpha");
        let projects = Project::all(&store.0.lock().unwrap());
        let found: Vec<(&str, &std::path::Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_deref().unwrap())).collect();
        assert_eq!(found, vec![("alpha", "/work/a".as_ref())]);
    }

    #[tokio::test]
    async fn a_second_init_in_the_same_root_fails() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);
        service.create_project(source("/work/a"), Actor::Human, name("a"), root("/work/a")).await.unwrap();

        let error = service.create_project(source("/work/a"), Actor::Human, name("other"), root("/work/a")).await.unwrap_err();

        assert_eq!(error.to_string(), "/work/a is already a project");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_init_with_a_taken_name_fails_whatever_its_case() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);
        service.create_project(source("/work/a"), Actor::Human, name("app"), root("/work/a")).await.unwrap();

        let error = service.create_project(source("/work/b"), Actor::Human, name("APP"), root("/work/b")).await.unwrap_err();

        assert_eq!(error.to_string(), "the name app is taken");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_broken_store_passes_through() {
        let broken = AppService::new(Broken, Skills).create_project(source("/work/a"), Actor::Human, name("a"), root("/work/a")).await;

        assert!(broken.is_err());
    }

    #[tokio::test]
    async fn a_web_project_has_no_root_and_is_recorded_from_the_web() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);

        service.create_project(Source::Web, Actor::Human, name("notes"), None).await.unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(Project::all(&events)[0].root, None);
        assert_eq!(events[0].source, Source::Web);
    }

    #[tokio::test]
    async fn a_rootless_project_still_owns_its_name() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);
        service.create_project(Source::Web, Actor::Human, name("notes"), None).await.unwrap();

        let web = service.create_project(Source::Web, Actor::Human, name("NOTES"), None).await.unwrap_err();
        let init = service.create_project(source("/work/a"), Actor::Human, name("notes"), root("/work/a")).await.unwrap_err();

        assert_eq!(web.to_string(), "the name notes is taken");
        assert!(web.is::<Taken>());
        assert!(init.is::<Taken>());
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rootless_projects_coexist() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);

        service.create_project(Source::Web, Actor::Human, name("one"), None).await.unwrap();
        service.create_project(Source::Web, Actor::Human, name("two"), None).await.unwrap();

        assert_eq!(store.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_path_outside_every_project_has_none() {
        let store = Memory::default();
        let service = AppService::new(&store, Skills);
        service.create_project(source("/work/app"), Actor::Human, name("app"), root("/work/app")).await.unwrap();
        service.create_project(Source::Web, Actor::Human, name("notes"), None).await.unwrap();

        let inside = service.project_at(std::path::Path::new("/work/app/src")).await.unwrap();
        let outside = service.project_at(std::path::Path::new("/elsewhere")).await.unwrap();

        assert_eq!(inside.map(|p| p.name), Some(name("app")));
        assert!(outside.is_none());
    }
}
