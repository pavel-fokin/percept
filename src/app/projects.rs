use super::AppService;
use crate::core::{Actor, DefinitionStore, Event, EventStore, Name, Project, ProjectId, Source};
use crate::shared::Result;

impl<S: EventStore, M: DefinitionStore> AppService<S, M> {
    /// Records a new project rooted at `source.path`, named `name`.
    /// A root holds one project, and a name belongs to one.
    pub async fn create_project(
        &self,
        source: Source,
        actor: Actor,
        name: Name,
    ) -> Result<Name> {
        // Held until return, so parallel inits cannot both pass the checks.
        let _lock = self.store.lock().await?;

        let projects = Project::all(&self.store.all().await?);
        if projects.iter().any(|p| p.root == source.path) {
            return Err(format!("{} is already a project", source.path.display()).into());
        }
        if projects.iter().any(|p| p.name == name) {
            return Err(format!("the name {} is taken", name.as_str()).into());
        }

        self.store.append(&Event::project_created(source.clone(), actor, ProjectId::new(), name.clone(), source.path)).await?;

        Ok(name)
    }

    /// Every project in `ProjectCreated` order.
    pub async fn projects(&self) -> Result<Vec<Project>> {
        Ok(Project::all(&self.store.all().await?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Definitions, Memory};

    fn name(name: &str) -> Name {
        Name::new(name).unwrap()
    }

    fn source(path: &str) -> Source {
        Source { client: "percept-cli".into(), path: path.into() }
    }

    #[tokio::test]
    async fn init_records_the_project_under_the_given_name() {
        let store = Memory::default();
        let service = AppService::new(&store, Definitions);

        let named = service.create_project(source("/work/a"), Actor::Human, name("Alpha")).await.unwrap();

        assert_eq!(named.as_str(), "alpha");
        let projects = Project::all(&store.0.lock().unwrap());
        let found: Vec<(&str, &std::path::Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_path())).collect();
        assert_eq!(found, vec![("alpha", "/work/a".as_ref())]);
    }

    #[tokio::test]
    async fn a_second_init_in_the_same_root_fails() {
        let store = Memory::default();
        let service = AppService::new(&store, Definitions);
        service.create_project(source("/work/a"), Actor::Human, name("a")).await.unwrap();

        let error = service.create_project(source("/work/a"), Actor::Human, name("other")).await.unwrap_err();

        assert_eq!(error.to_string(), "/work/a is already a project");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_init_with_a_taken_name_fails_whatever_its_case() {
        let store = Memory::default();
        let service = AppService::new(&store, Definitions);
        service.create_project(source("/work/a"), Actor::Human, name("app")).await.unwrap();

        let error = service.create_project(source("/work/b"), Actor::Human, name("APP")).await.unwrap_err();

        assert_eq!(error.to_string(), "the name app is taken");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_broken_store_passes_through() {
        let broken = AppService::new(Broken, Definitions).create_project(source("/work/a"), Actor::Human, name("a")).await;

        assert!(broken.is_err());
    }
}
