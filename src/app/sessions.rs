use std::collections::BTreeMap;

use jiff::Timestamp;

use super::AppService;
use crate::core::{Actor, ClientSessionId, DefinitionStore, Event, EventStore, Payload, Project, Session, SessionId, SessionStatus, Source};
use crate::shared::Result;

impl<S: EventStore, M: DefinitionStore> AppService<S, M> {
    /// Sessions newest first.
    pub async fn sessions(&self) -> Result<Vec<Session>> {
        let events = self.store.all().await?;

        let mut titles: BTreeMap<SessionId, &str> = BTreeMap::new();
        for e in events.iter().filter(|e| e.actor == Actor::Human) {
            if let Payload::Message { session, content } = &e.payload
                && !content.trim().is_empty()
            {
                titles.entry(*session).or_insert(content);
            }
        }

        let mut last: BTreeMap<SessionId, (SessionStatus, Timestamp)> = BTreeMap::new();
        for e in &events {
            if let (Some(session), Some(status)) = (e.payload.session(), SessionStatus::after(e)) {
                last.insert(session, (status, e.created_at));
            }
        }

        let sessions = events
            .iter()
            .rev()
            .filter_map(|e| match &e.payload {
                Payload::SessionCreated { session, project, client_session_id } => {
                    let (status, last_event_at) = last[session];
                    Some(Session {
                        id: *session,
                        client_session_id: client_session_id.clone(),
                        project: *project,
                        created_at: e.created_at,
                        client: e.source.client.clone(),
                        title: titles.get(session).map(|t| t.to_string()),
                        status,
                        last_event_at,
                    })
                }
                _ => None,
            })
            .collect();

        Ok(sessions)
    }

    pub async fn session(&self, id: SessionId) -> Result<Option<Session>> {
        Ok(self.sessions().await?.into_iter().find(|s| s.id == id))
    }

    /// The session's events in log order, or `None` if the session does not exist.
    pub async fn session_events(&self, id: SessionId) -> Result<Option<Vec<Event>>> {
        let events: Vec<Event> = self.store.all().await?.into_iter().filter(|e| e.payload.session() == Some(id)).collect();

        let exists = events.iter().any(|e| matches!(e.payload, Payload::SessionCreated { .. }));
        Ok(exists.then_some(events))
    }

    /// Appends the event to the session the client's session id names, creating the session first if it is new.
    /// A new session joins the project nearest above `source.path`; with none, nothing is recorded.
    pub async fn record(
        &self,
        client_session_id: ClientSessionId,
        source: Source,
        event: impl FnOnce(Source, SessionId) -> Event,
    ) -> Result<()> {
        // Held until return, so parallel hooks cannot create the same session twice.
        let _lock = self.store.lock().await?;

        let session = match self.store.session(&client_session_id).await? {
            Some(session) => session,
            None => {
                let Some(project) = Project::containing(&self.store.all().await?, &source.path) else {
                    return Ok(());
                };

                let session = SessionId::new();
                let created = Event::session_created(source.clone(), session, project.id, client_session_id);
                self.store.append(&created).await?;

                session
            }
        };

        self.store.append(&event(source, session)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::{Broken, Definitions, in_project};
    use crate::core::Kind;

    use serde_json::json;

    fn client_session_id(id: &str) -> ClientSessionId {
        ClientSessionId::new(id.into())
    }

    #[tokio::test]
    async fn passes_store_errors_through() {
        assert!(AppService::new(Broken, Definitions).sessions().await.is_err());
        assert!(AppService::new(Broken, Definitions).session(SessionId::new()).await.is_err());
        assert!(AppService::new(Broken, Definitions).session_events(SessionId::new()).await.is_err());
    }

    #[tokio::test]
    async fn sessions_are_the_created_events_newest_first() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let events = store.0.lock().unwrap();
        let found: Vec<(SessionId, &str)> = sessions.iter().map(|s| (s.id, s.client_session_id.as_str())).collect();
        assert_eq!(found, vec![(events[3].payload.session().unwrap(), "b"), (events[1].payload.session().unwrap(), "a")]);
        assert_eq!(sessions[1].created_at, events[1].created_at);
    }

    #[tokio::test]
    async fn session_finds_one_session_by_id() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[1].payload.session().unwrap();

        let found = service.session(id).await.unwrap().unwrap();

        assert_eq!(found.id, id);
        assert_eq!(found.client_session_id.as_str(), "a");
        assert_eq!(found.title.as_deref(), Some("hi"));
        assert!(service.session(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn session_events_are_the_sessions_own_in_log_order() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Read", None, None, json!({}))).await.unwrap();
        let id = store.0.lock().unwrap()[1].payload.session().unwrap();

        let events = service.session_events(id).await.unwrap().unwrap();

        let kinds: Vec<Kind> = events.iter().map(|e| e.payload.kind()).collect();
        assert_eq!(kinds, vec![Kind::SessionCreated, Kind::Message, Kind::ToolUsed]);
        assert!(service.session_events(SessionId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn client_is_the_source_client_of_the_created_event() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        let source = Source { client: "claude-code".into(), path: "/work/app".into() };
        service.record(client_session_id("a"), source, |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].client, "claude-code");
    }

    #[tokio::test]
    async fn title_is_the_first_human_message() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "agent", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "first", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "second", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title.as_deref(), Some("first"));
    }

    #[tokio::test]
    async fn title_skips_empty_human_messages() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, " ", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "first", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title.as_deref(), Some("first"));
    }

    #[tokio::test]
    async fn title_is_none_without_a_human_message() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "agent", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].title, None);
    }

    #[tokio::test]
    async fn status_is_ended_after_session_stopped() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::session_stopped(src, s, json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].status, SessionStatus::Ended);
    }

    #[tokio::test]
    async fn status_is_waiting_after_an_agent_message() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "done", json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].status, SessionStatus::Waiting);
    }

    #[tokio::test]
    async fn status_is_working_mid_turn() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Agent, "done", json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "next", json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        assert_eq!(sessions[0].status, SessionStatus::Working);
        assert_eq!(sessions[1].status, SessionStatus::Working);
    }

    #[tokio::test]
    async fn last_event_at_is_the_time_of_the_last_event() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Read", None, None, json!({}))).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(sessions[1].last_event_at, events[5].created_at);
        assert_eq!(sessions[0].last_event_at, events[4].created_at);
    }

    #[tokio::test]
    async fn record_creates_a_session_for_an_unknown_client_session_id() {
        let store = in_project("/");

        AppService::new(&store, Definitions)
            .record(client_session_id("a"), Source::fixture(), |src, s| Event::message(src, s, Actor::Human, "hi", json!({})))
            .await
            .unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[1].payload,
            Payload::SessionCreated { client_session_id, .. } if client_session_id.as_str() == "a"
        ));
        assert!(matches!(&events[2].payload, Payload::Message { content, .. } if content == "hi"));
        assert_eq!(events[2].payload.session(), events[1].payload.session());
    }

    #[tokio::test]
    async fn record_reuses_the_session_of_a_known_client_session_id() {
        let store = in_project("/");
        let service = AppService::new(&store, Definitions);

        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();
        service.record(client_session_id("b"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({}))).await.unwrap();

        let events = store.0.lock().unwrap();
        let kinds: Vec<Kind> = events.iter().map(|e| e.payload.kind()).collect();
        assert_eq!(
            kinds,
            vec![Kind::ProjectCreated, Kind::SessionCreated, Kind::ToolUsed, Kind::ToolUsed, Kind::SessionCreated, Kind::ToolUsed]
        );
        assert_eq!(events[2].payload.session(), events[1].payload.session());
        assert_eq!(events[3].payload.session(), events[1].payload.session());
        assert_ne!(events[5].payload.session(), events[1].payload.session());
    }

    #[tokio::test]
    async fn record_gives_the_session_created_event_the_same_source_and_no_later_a_time() {
        let store = in_project("/");
        let source = Source { client: "codex".into(), path: "/work/app".into() };

        AppService::new(&store, Definitions)
            .record(client_session_id("a"), source.clone(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({})))
            .await
            .unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events[1].source, source);
        assert_eq!(events[2].source, source);
        assert!(events[1].id <= events[2].id);
        assert!(events[1].created_at <= events[2].created_at);
    }

    fn at(path: &str) -> Source {
        Source { client: "claude-code".into(), path: path.into() }
    }

    fn tool(src: Source, s: SessionId) -> Event {
        Event::tool_used(src, s, "Bash", None, None, json!({}))
    }

    #[tokio::test]
    async fn a_session_joins_the_project_of_its_root_a_subfolder_or_a_nested_worktree() {
        let store = in_project("/work/app");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), at("/work/app"), tool).await.unwrap();
        service.record(client_session_id("b"), at("/work/app/src/deep"), tool).await.unwrap();
        service.record(client_session_id("c"), at("/work/app/.claude/worktrees/x"), tool).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let project = Project::all(&store.0.lock().unwrap())[0].id;
        assert_eq!(sessions.len(), 3);
        assert!(sessions.iter().all(|s| s.project == project));
    }

    #[tokio::test]
    async fn a_session_joins_the_nearest_of_two_nested_projects() {
        let store = in_project("/work/app");
        store.0.lock().unwrap().push(in_project("/work/app/inner").0.lock().unwrap()[0].clone());
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), at("/work/app/inner/src"), tool).await.unwrap();
        service.record(client_session_id("b"), at("/work/app/src"), tool).await.unwrap();

        let sessions = service.sessions().await.unwrap();

        let projects = Project::all(&store.0.lock().unwrap());
        assert_eq!(sessions[0].project, projects[0].id);
        assert_eq!(sessions[1].project, projects[1].id);
    }

    #[tokio::test]
    async fn a_hook_outside_every_project_records_nothing() {
        let store = in_project("/work/app");
        let service = AppService::new(&store, Definitions);

        service.record(client_session_id("a"), at("/work/apple"), tool).await.unwrap();

        assert_eq!(store.0.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_known_session_keeps_its_project_and_its_events_are_recorded_from_anywhere() {
        let store = in_project("/work/app");
        let service = AppService::new(&store, Definitions);
        service.record(client_session_id("a"), at("/work/app"), tool).await.unwrap();

        service.record(client_session_id("a"), at("/elsewhere"), tool).await.unwrap();

        let events = store.0.lock().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[3].payload.session(), events[1].payload.session());
    }

    #[tokio::test]
    async fn record_passes_store_errors_through() {
        let result = AppService::new(Broken, Definitions)
            .record(client_session_id("a"), Source::fixture(), |src, s| Event::tool_used(src, s, "Bash", None, None, json!({})))
            .await;

        assert!(result.is_err());
    }
}
