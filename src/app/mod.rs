use std::sync::Arc;

use crate::percept::{self, Event, Sender};

/// Streams the reply chunk by chunk; returning means it's done.
pub type ReplyStream = Box<dyn FnOnce(&mut dyn FnMut(String)) + Send>;

/// What tui needs from the app layer. Lives here, not in tui, so
/// implementing it doesn't pull tui into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a thunk that computes the
    /// reply. The thunk captures its own history snapshot, so it's safe
    /// to run off-thread.
    fn submit(&mut self, text: String) -> ReplyStream;

    /// Appends a chunk to the in-progress reply, starting a new
    /// assistant event on the first chunk. Call only from the task that
    /// owns the Conversation, never from inside the thunk.
    fn append_chunk(&mut self, content: String);

    /// Marks the reply complete, so the next chunk starts a new
    /// assistant event instead of extending this one.
    fn end_stream(&mut self);

    fn events(&self) -> &[Event];
}

/// Orchestrates a chat: turns input into events, asks Model for a
/// reply, keeps the transcript.
pub struct Conversation {
    events: Vec<Event>,
    chat: Arc<dyn percept::Model>,
    /// Index of the event currently receiving chunks, or None if no
    /// stream is in progress.
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
