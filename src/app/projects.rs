use super::AppService;
use crate::core::{Actor, Event, EventStore, Name, Project, ProjectId, SchemaStore, Source};
use crate::shared::Result;

impl<S: EventStore, M: SchemaStore> AppService<S, M> {
    /// Records a new project rooted at `source.path`, named `name` or, when none is given, after its folder.
    /// A root holds one project, and a name belongs to one.
    pub async fn create_project(
        &self,
        source: Source,
        actor: Actor,
        name: Option<&str>,
    ) -> Result<Name> {
        let name = match name {
            Some(name) => Name::new(name)?,
            None => Name::from_root(&source.path)?,
        };

        // Held until return, so parallel inits cannot both pass the checks.
        let _lock = self.store.lock().await?;

        let projects = Project::all(&self.store.all().await?);
        if projects.iter().any(|p| p.root == source.path) {
            return Err(format!("{} is already a project", source.path.display()).into());
        }
        if projects.iter().any(|p| p.name == name) {
            return Err(format!("the name {} is taken", name.as_str()).into());
        }

        self.store.append(&Event::project_created(source, actor, ProjectId::new(), name.clone())).await?;

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
    use crate::app::testing::{Broken, Memory, Schemas};

    fn source(path: &str) -> Source {
        Source { client: "percept-cli".into(), path: path.into() }
    }

    #[tokio::test]
    async fn init_records_the_project_under_the_given_or_the_folders_name() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);

        let named = service.create_project(source("/work/a"), Actor::Human, Some("Alpha")).await.unwrap();
        let default = service.create_project(source("/work/My App"), Actor::Human, None).await.unwrap();

        assert_eq!((named.as_str(), default.as_str()), ("alpha", "my-app"));
        let projects = Project::all(&store.0.lock().unwrap());
        let found: Vec<(&str, &std::path::Path)> = projects.iter().map(|p| (p.name.as_str(), p.root.as_path())).collect();
        assert_eq!(found, vec![("alpha", "/work/a".as_ref()), ("my-app", "/work/My App".as_ref())]);
    }

    #[tokio::test]
    async fn a_second_init_in_the_same_root_fails() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_project(source("/work/a"), Actor::Human, None).await.unwrap();

        let error = service.create_project(source("/work/a"), Actor::Human, Some("other")).await.unwrap_err();

        assert_eq!(error.to_string(), "/work/a is already a project");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_init_with_a_taken_name_fails_whatever_its_case() {
        let store = Memory::default();
        let service = AppService::new(&store, Schemas);
        service.create_project(source("/work/a"), Actor::Human, Some("app")).await.unwrap();

        let error = service.create_project(source("/work/b"), Actor::Human, Some("APP")).await.unwrap_err();

        assert_eq!(error.to_string(), "the name app is taken");
        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_invalid_name_fails_and_a_broken_store_passes_through() {
        let store = Memory::default();

        let invalid = AppService::new(&store, Schemas).create_project(source("/work/a"), Actor::Human, Some("a b")).await;
        let broken = AppService::new(Broken, Schemas).create_project(source("/work/a"), Actor::Human, None).await;

        assert!(invalid.is_err());
        assert!(broken.is_err());
        assert!(store.0.lock().unwrap().is_empty());
    }
}
