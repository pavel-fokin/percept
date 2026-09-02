use std::sync::Arc;

use crate::percept::{self, Actor, Event, EventId};

/// What tui needs from the app layer. Lives here, not in tui, so
/// implementing it doesn't pull tui into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a stream of the reply's
    /// chunks. Errs, without recording anything, if a turn is already
    /// streaming or if the event can't be appended to the log.
    fn submit(&mut self, text: String) -> Result<percept::ReplyStream, Box<dyn std::error::Error>>;

    /// Appends a chunk - thought or reply text - to the in-progress
    /// turn. Neither is an event yet - both are committed once by
    /// `end_stream`. Call only from the task that owns the App, never
    /// from inside the task draining the stream.
    fn append_chunk(&mut self, chunk: percept::Chunk);

    /// Commits the streamed thought, if any, then the streamed reply, if
    /// any, as separate model events. Either with no chunks commits
    /// nothing. Errs if an event can't be appended to the log; a failed
    /// thought append leaves the reply uncommitted too.
    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>>;

    fn events(&self) -> &[Event];

    /// The reply now streaming, if any - not yet in `events`.
    fn pending_reply(&self) -> Option<&str>;

    /// The thought now streaming, if any - not yet in `events`.
    fn pending_thought(&self) -> Option<&str>;

    /// Whether a turn is still streaming. A second `submit` before it
    /// ends would overwrite the first turn's cause and fuse both
    /// replies into one event, and an append-only log keeps the damage.
    fn is_replying(&self) -> bool;
}

/// The turn now streaming: what caused it, and the text arriving for
/// each event it will commit. One value, so the cause can't outlive the
/// buffers it belongs to.
struct Turn {
    cause: EventId,
    thought: String,
    reply: String,
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
    /// The turn now streaming, or None between turns.
    pending: Option<Turn>,
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
        })
    }

    /// Appends an event, then adds it to the transcript - never the
    /// other way round, so a failed write can't leave the transcript
    /// ahead of what's durable.
    fn commit(&mut self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        self.log.append(&event)?;
        self.events.push(event);
        Ok(())
    }
}

impl AppService for App {
    fn submit(&mut self, text: String) -> Result<percept::ReplyStream, Box<dyn std::error::Error>> {
        if self.pending.is_some() {
            return Err("a reply is already streaming".into());
        }
        let event = Event::message_received(Actor::User, text, self.source.clone(), None);
        self.log.append(&event)?;
        self.pending = Some(Turn {
            cause: event.id(),
            thought: String::new(),
            reply: String::new(),
        });
        self.events.push(event);

        let history = percept::to_messages(&self.events);
        Ok(self.chat.reply(&history))
    }

    fn append_chunk(&mut self, chunk: percept::Chunk) {
        let Some(turn) = self.pending.as_mut() else {
            return;
        };
        match chunk {
            percept::Chunk::Thought(text) => turn.thought.push_str(&text),
            percept::Chunk::Reply(text) => turn.reply.push_str(&text),
        }
    }

    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(turn) = self.pending.as_ref() else {
            return Ok(());
        };
        let cause = Some(turn.cause);
        let thought = turn.thought.clone();
        let reply = turn.reply.clone();

        // A buffer is only cleared once its event is durable. Taking
        // the text first would leave a failed append with nothing to
        // retry from. The thought commits before the reply - the prompt
        // caused both, but the thought came first.
        if !thought.is_empty() {
            let event = Event::thought_recorded(Actor::Model, thought, self.source.clone(), cause);
            self.commit(event)?;
            if let Some(turn) = self.pending.as_mut() {
                turn.thought.clear();
            }
        }
        if !reply.is_empty() {
            let event = Event::message_received(Actor::Model, reply, self.source.clone(), cause);
            self.commit(event)?;
        }
        self.pending = None;
        Ok(())
    }

    fn events(&self) -> &[Event] {
        &self.events
    }

    fn pending_reply(&self) -> Option<&str> {
        text(self.pending.as_ref().map(|turn| &turn.reply))
    }

    fn pending_thought(&self) -> Option<&str> {
        text(self.pending.as_ref().map(|turn| &turn.thought))
    }

    fn is_replying(&self) -> bool {
        self.pending.is_some()
    }
}

/// A buffer that has taken no chunks yet reads as nothing streaming,
/// so a caller never renders an empty turn.
fn text(buffer: Option<&String>) -> Option<&str> {
    buffer.map(String::as_str).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Chunk, Message, Payload};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    const SOURCE: &str = "tui";

    struct Silent;

    impl percept::Model for Silent {
        fn capabilities(&self) -> percept::ModelCapabilities {
            percept::ModelCapabilities {
                input: &[percept::Modality::Text],
                output: &[percept::Modality::Text],
                tool_use: false,
            }
        }

        fn reply(&self, _messages: &[Message]) -> percept::ReplyStream {
            Box::pin(tokio_stream::empty())
        }
    }

    fn content(event: &Event) -> &str {
        match event.payload() {
            Payload::MessageReceived { content } => content,
            _ => panic!("expected a message.received event"),
        }
    }

    fn thought(event: &Event) -> &str {
        match event.payload() {
            Payload::ThoughtRecorded { content } => content,
            _ => panic!("expected a thought.recorded event"),
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

        fn get(&self, id: percept::EventId) -> Result<Option<Event>, Box<dyn std::error::Error>> {
            let events = self.events.lock().unwrap();
            Ok(events.iter().find(|event| event.id() == id).cloned())
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

        app.append_chunk(Chunk::Reply("he".to_string()));
        app.append_chunk(Chunk::Reply("llo".to_string()));
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
    fn a_thought_and_a_reply_commit_as_two_model_events_thought_first() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hmm".to_string()));
        app.append_chunk(Chunk::Reply("hello".to_string()));
        assert_eq!(app.pending_thought(), Some("hmm"));
        assert_eq!(app.pending_reply(), Some("hello"));

        app.end_stream().unwrap();
        assert!(app.pending_thought().is_none());
        assert!(app.pending_reply().is_none());

        let events = app.events();
        assert_eq!(events.len(), 3);
        assert!(events[1].actor() == Actor::Model);
        assert_eq!(thought(&events[1]), "hmm");
        assert!(events[2].actor() == Actor::Model);
        assert_eq!(content(&events[2]), "hello");
        assert!(events[1].causation_id() == Some(events[0].id()));
        assert!(events[2].causation_id() == Some(events[0].id()));
    }

    #[test]
    fn a_submit_while_a_turn_streams_is_refused_and_records_nothing() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("first".to_string()).unwrap();
        assert!(app.is_replying());
        assert!(app.submit("second".to_string()).is_err());
        assert_eq!(app.events().len(), 1);

        app.append_chunk(Chunk::Reply("done".to_string()));
        app.end_stream().unwrap();
        assert!(!app.is_replying());
        assert!(app.submit("second".to_string()).is_ok());
    }

    #[test]
    fn a_turn_with_a_thought_and_no_reply_still_ends() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hm".to_string()));
        app.end_stream().unwrap();

        assert!(!app.is_replying());
        assert_eq!(app.events().len(), 2);
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
        app.append_chunk(Chunk::Reply("hello".to_string()));
        log.start_failing();

        assert!(app.end_stream().is_err());
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn a_failed_thought_append_leaves_the_reply_unattempted() {
        let log = Arc::new(FakeLog::default());
        let mut app = App::new(Arc::new(Silent), log.clone(), SOURCE.to_string()).unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hmm".to_string()));
        app.append_chunk(Chunk::Reply("hello".to_string()));
        log.start_failing();

        assert!(app.end_stream().is_err());
        assert_eq!(app.pending_thought(), Some("hmm"));
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);
    }
}
