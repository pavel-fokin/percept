use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::percept::{self, Actor, Event, EventId};

/// Process-global event sequence. Gap-free and monotonic across every
/// Conversation in the process, assigned when an event is committed.
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

fn next_seq() -> u64 {
    EVENT_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Streams the reply chunk by chunk; returning means it's done.
pub type ReplyStream = Box<dyn FnOnce(&mut dyn FnMut(String)) + Send>;

/// What tui needs from the app layer. Lives here, not in tui, so
/// implementing it doesn't pull tui into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a thunk that computes the
    /// reply. The thunk captures its own history snapshot, so it's safe
    /// to run off-thread.
    fn submit(&mut self, text: String) -> ReplyStream;

    /// Appends a chunk to the in-progress reply. The reply isn't an
    /// event yet - it's committed once by `end_stream`. Call only from
    /// the task that owns the Conversation, never from inside the thunk.
    fn append_chunk(&mut self, content: String);

    /// Commits the streamed reply as one assistant event. A reply with
    /// no chunks commits nothing.
    fn end_stream(&mut self);

    fn events(&self) -> &[Event];

    /// The reply now streaming, if any - not yet in `events`.
    fn pending_reply(&self) -> Option<&str>;
}

/// Orchestrates a chat: turns input into events, asks Model for a
/// reply, keeps the transcript.
pub struct Conversation {
    events: Vec<Event>,
    chat: Arc<dyn percept::Model>,
    /// Text of the reply now streaming, or None between replies.
    pending: Option<String>,
    /// The user message the streaming reply answers - its `causation_id`
    /// once committed.
    pending_cause: Option<EventId>,
}

impl Conversation {
    pub fn new(chat: Arc<dyn percept::Model>) -> Self {
        Self {
            events: Vec::new(),
            chat,
            pending: None,
            pending_cause: None,
        }
    }
}

impl AppService for Conversation {
    fn submit(&mut self, text: String) -> ReplyStream {
        let event = Event::message_received(Actor::User, text, next_seq(), None);
        self.pending_cause = Some(event.id());
        self.events.push(event);

        let history = percept::to_messages(&self.events);
        let chat = Arc::clone(&self.chat);
        Box::new(move |on_chunk: &mut dyn FnMut(String)| {
            if chat.reply(&history, on_chunk).is_err() {
                on_chunk("Sorry, something went wrong.".to_string());
            }
        })
    }

    fn append_chunk(&mut self, content: String) {
        self.pending
            .get_or_insert_with(String::new)
            .push_str(&content);
    }

    fn end_stream(&mut self) {
        if let Some(content) = self.pending.take() {
            let event = Event::message_received(
                Actor::Model,
                content,
                next_seq(),
                self.pending_cause.take(),
            );
            self.events.push(event);
        }
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
        }
    }

    #[test]
    fn streamed_reply_commits_one_event_caused_by_the_prompt() {
        let mut convo = Conversation::new(Arc::new(Silent));

        let _ = convo.submit("hi".to_string());
        assert_eq!(convo.events().len(), 1);
        assert!(convo.pending_reply().is_none());

        convo.append_chunk("he".to_string());
        convo.append_chunk("llo".to_string());
        assert_eq!(convo.pending_reply(), Some("hello"));
        assert_eq!(convo.events().len(), 1);

        convo.end_stream();
        assert!(convo.pending_reply().is_none());

        let events = convo.events();
        assert_eq!(events.len(), 2);
        assert!(events[0].actor() == Actor::User);
        assert!(events[1].actor() == Actor::Model);
        assert_eq!(content(&events[1]), "hello");
        assert!(events[1].causation_id() == Some(events[0].id()));
        assert!(events[1].seq() > events[0].seq());
    }

    #[test]
    fn empty_reply_commits_nothing() {
        let mut convo = Conversation::new(Arc::new(Silent));
        let _ = convo.submit("hi".to_string());
        convo.end_stream();
        assert_eq!(convo.events().len(), 1);
    }
}
