use crate::percept::{self, Event, Sender};

/// What tui needs from the application layer. Owned here, not in tui:
/// Rust requires the implementor to import a trait to name it in an impl
/// block, so putting this in tui would force an app -> tui edge. tui
/// depends on this trait instead of the concrete Conversation type.
pub trait AppService {
    fn submit(&mut self, text: String) -> Result<(), Box<dyn std::error::Error>>;
    fn events(&self) -> &[Event];
}

/// Conversation orchestrates a chat: turns input into domain events, asks
/// the configured Model for a reply, keeps the transcript. Pure
/// orchestration - no vocabulary beyond percept's.
pub struct Conversation {
    events: Vec<Event>,
    chat: Box<dyn percept::Model>,
}

impl Conversation {
    pub fn new(chat: Box<dyn percept::Model>) -> Self {
        Self {
            events: Vec::new(),
            chat,
        }
    }
}

impl AppService for Conversation {
    fn submit(&mut self, text: String) -> Result<(), Box<dyn std::error::Error>> {
        self.events.push(Event::new(Sender::User, text));

        let history = percept::to_messages(&self.events);
        let reply = self
            .chat
            .reply(&history)
            .unwrap_or_else(|_| "Sorry, something went wrong.".to_string());

        self.events.push(Event::new(Sender::Assistant, reply));
        Ok(())
    }

    fn events(&self) -> &[Event] {
        &self.events
    }
}
