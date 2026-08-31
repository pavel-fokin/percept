use std::sync::Arc;

use crate::percept::{self, Actor, Event, EventId};

/// Streams the reply chunk by chunk; returning means it's done.
pub type ReplyStream = Box<dyn FnOnce(&mut dyn FnMut(String)) + Send>;

/// What tui needs from the app layer. Lives here, not in tui, so
/// implementing it doesn't pull tui into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a thunk that computes the
    /// reply. The thunk captures its own history snapshot, so it's safe
    /// to run off-thread. Errs, without recording anything, if the
    /// event can't be appended to the log.
    fn submit(&mut self, text: String) -> Result<ReplyStream, Box<dyn std::error::Error>>;

    /// Appends a chunk to the in-progress reply. The reply isn't an
    /// event yet - it's committed once by `end_stream`. Call only from
    /// the task that owns the App, never from inside the thunk.
    fn append_chunk(&mut self, content: String);

    /// Commits the streamed reply as one assistant event. A reply with
    /// no chunks commits nothing. Errs if the event can't be appended
    /// to the log.
    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>>;

    fn events(&self) -> &[Event];

    /// The reply now streaming, if any - not yet in `events`.
    fn pending_reply(&self) -> Option<&str>;
}

/// Orchestrates a chat: turns input into events, asks Model for a
/// reply, keeps the transcript. Every event goes through `log` before
/// it's added to `events`, so a failed write can never leave the
/// in-memory transcript ahead of what's durable.
pub struct App {
    events: Vec<Event>,
    /// The writer this app records as - stamped on every event it
    /// commits, so the log can tell its events from other writers'.
    source: String,
    chat: Arc<dyn percept::Model>,
    log: Arc<dyn percept::EventLog>,
    /// Text of the reply now streaming, or None between replies.
    pending: Option<String>,
    /// The user message the streaming reply answers - its `causation_id`
    /// once committed.
    pending_cause: Option<EventId>,
}

impl App {
    /// Opens on whatever `log` already holds, so the transcript
    /// survives a restart.
    pub fn new(
        chat: Arc<dyn percept::Model>,
        log: Arc<dyn percept::EventLog>,
        source: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let events = log.load()?;

        Ok(Self {
            events,
            source,
            chat,
            log,
            pending: None,
            pending_cause: None,
        })
    }
}

impl AppService for App {
    fn submit(&mut self, text: String) -> Result<ReplyStream, Box<dyn std::error::Error>> {
        let event = Event::message_received(Actor::User, text, self.source.clone(), None);
        self.log.append(&event)?;
        self.pending_cause = Some(event.id());
        self.events.push(event);

        let history = percept::to_messages(&self.events);
        let chat = Arc::clone(&self.chat);
        Ok(Box::new(move |on_chunk: &mut dyn FnMut(String)| {
            if chat.reply(&history, on_chunk).is_err() {
                on_chunk("Sorry, something went wrong.".to_string());
            }
        }))
    }

    fn append_chunk(&mut self, content: String) {
        self.pending
            .get_or_insert_with(String::new)
            .push_str(&content);
    }

    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(content) = self.pending.as_ref() else {
            return Ok(());
        };
        // The reply is only cleared once it's durable. Taking it first
        // would leave a failed append with nothing to retry from.
        let event = Event::message_received(
            Actor::Model,
            content.clone(),
            self.source.clone(),
            self.pending_cause,
        );
        self.log.append(&event)?;
        self.pending = None;
        self.pending_cause = None;
        self.events.push(event);
        Ok(())
    }

    fn events(&self) -> &[Event] {
        &self.events
    }

    fn pending_reply(&self) -> Option<&str> {
        self.pending.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Message, Payload};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    const SOURCE: &str = "tui";

    struct Silent;

    impl percept::Model for Silent {
        fn reply(
            &self,
            _messages: &[Message],
            _on_chunk: &mut dyn FnMut(String),
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }
    }

    fn content(event: &Event) -> &str {
        match event.payload() {
            Payload::MessageReceived { content } => content,
            Payload::ToolUsed { .. } => panic!("expected a message.received event"),
        }
    }

    /// An in-memory EventLog for tests. `fail_append` flips `append`
    /// into an error without touching the filesystem, and can be
    /// flipped mid-conversation.
    #[derive(Default)]
    struct FakeLog {
        events: Mutex<Vec<Event>>,
        fail_append: AtomicBool,
    }

    impl FakeLog {
        fn seeded(events: Vec<Event>) -> Self {
            Self {
                events: Mutex::new(events),
                ..Self::default()
            }
        }

        fn start_failing(&self) {
            self.fail_append.store(true, Ordering::Relaxed);
        }
    }

    impl percept::EventLog for FakeLog {
        fn append(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
            if self.fail_append.load(Ordering::Relaxed) {
                return Err("append failed".into());
            }
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }

        fn load(&self) -> Result<Vec<Event>, Box<dyn std::error::Error>> {
            Ok(self.events.lock().unwrap().clone())
        }
    }

    #[test]
    fn streamed_reply_commits_one_event_caused_by_the_prompt() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        assert_eq!(app.events().len(), 1);
        assert!(app.pending_reply().is_none());

        app.append_chunk("he".to_string());
        app.append_chunk("llo".to_string());
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);

        app.end_stream().unwrap();
        assert!(app.pending_reply().is_none());

        let events = app.events();
        assert_eq!(events.len(), 2);
        assert!(events[0].actor() == Actor::User);
        assert!(events[1].actor() == Actor::Model);
        assert_eq!(content(&events[1]), "hello");
        assert!(events[1].causation_id() == Some(events[0].id()));
        assert_eq!(events[0].source(), SOURCE);
        assert_eq!(events[1].source(), SOURCE);
    }

    #[test]
    fn empty_reply_commits_nothing() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            SOURCE.to_string(),
        )
        .unwrap();
        let _ = app.submit("hi".to_string()).unwrap();
        app.end_stream().unwrap();
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn preseeded_log_becomes_the_opening_transcript() {
        let seeded = vec![
            Event::message_received(Actor::User, "hi".to_string(), SOURCE.to_string(), None),
            Event::message_received(Actor::Model, "hello".to_string(), SOURCE.to_string(), None),
        ];
        let log = Arc::new(FakeLog::seeded(seeded));
        let mut app = App::new(Arc::new(Silent), log, SOURCE.to_string()).unwrap();
        assert_eq!(app.events().len(), 2);

        let _ = app.submit("next".to_string()).unwrap();
        assert_eq!(app.events().len(), 3);
    }

    #[test]
    fn append_failure_surfaces_as_err_and_leaves_transcript_unchanged() {
        let log = Arc::new(FakeLog::default());
        log.start_failing();
        let mut app = App::new(Arc::new(Silent), log.clone(), SOURCE.to_string()).unwrap();

        assert!(app.submit("hi".to_string()).is_err());
        assert!(app.events().is_empty());
    }

    #[test]
    fn a_failed_reply_append_leaves_the_reply_pending() {
        let log = Arc::new(FakeLog::default());
        let mut app = App::new(Arc::new(Silent), log.clone(), SOURCE.to_string()).unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk("hello".to_string());
        log.start_failing();

        assert!(app.end_stream().is_err());
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);
    }
}
