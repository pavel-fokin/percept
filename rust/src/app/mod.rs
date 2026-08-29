use std::sync::Arc;

use crate::percept::{self, Event, Sender};

/// A thunk that streams the assistant's reply, invoking the given
/// callback once per chunk; returning marks the reply complete.
pub type ReplyStream = Box<dyn FnOnce(&mut dyn FnMut(String)) + Send>;

/// What tui needs from the application layer. Owned here, not in tui:
/// Rust requires the implementor to import a trait to name it in an impl
/// block, so putting this in tui would force an app -> tui edge. tui
/// depends on this trait instead of the concrete Conversation type.
pub trait AppService {
    /// Records the user's message and returns a thunk that streams the
    /// assistant's reply. The history it needs is captured now, before
    /// returning - the thunk borrows nothing, so it's safe to run on any
    /// thread (e.g. via `spawn_blocking`).
    fn submit(&mut self, text: String) -> ReplyStream;

    /// Appends a chunk to the in-progress assistant reply, starting a
    /// new assistant event on the first chunk of a stream. Must only be
    /// called from the task that owns the Conversation - never from
    /// inside the thunk.
    fn append_chunk(&mut self, content: String);

    /// Marks the in-progress reply complete, so the next chunk received
    /// starts a new assistant event instead of extending this one.
    fn end_stream(&mut self);

    fn events(&self) -> &[Event];
}

/// Conversation orchestrates a chat: turns input into domain events, asks
/// the configured Model for a reply, keeps the transcript. Pure
/// orchestration - no vocabulary beyond percept's.
pub struct Conversation {
    events: Vec<Event>,
    chat: Arc<dyn percept::Model>,
    /// Index into events of the assistant event currently receiving
    /// chunks, or None if no stream is in progress.
    streaming: Option<usize>,
}

impl Conversation {
    pub fn new(chat: Arc<dyn percept::Model>) -> Self {
        Self {
            events: Vec::new(),
            chat,
            streaming: None,
        }
    }
}

impl AppService for Conversation {
    fn submit(&mut self, text: String) -> ReplyStream {
        self.events.push(Event::new(Sender::User, text));

        let history = percept::to_messages(&self.events);
        let chat = Arc::clone(&self.chat);
        Box::new(move |on_chunk: &mut dyn FnMut(String)| {
            if chat.reply(&history, on_chunk).is_err() {
                on_chunk("Sorry, something went wrong.".to_string());
            }
        })
    }

    fn append_chunk(&mut self, content: String) {
        match self.streaming {
            Some(idx) => self.events[idx].content.push_str(&content),
            None => {
                self.events.push(Event::new(Sender::Assistant, content));
                self.streaming = Some(self.events.len() - 1);
            }
        }
    }

    fn end_stream(&mut self) {
        self.streaming = None;
    }

    fn events(&self) -> &[Event] {
        &self.events
    }
}
