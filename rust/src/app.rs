use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

use crate::event::{Event, Sender};
use crate::llm::{Message, Model, Role};
use crate::providers::Stub;

pub struct App<'a> {
    pub events: Vec<Event>,
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
    chat: Box<dyn Model>,
}

impl<'a> App<'a> {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            textarea: new_textarea(),
            user_style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            assistant_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            chat: Box::new(Stub),
        }
    }

    /// Appends the pending input as a user event, asks the configured chat
    /// model for a reply, then appends that as an assistant event before
    /// clearing the input.
    pub fn submit(&mut self) {
        let text = self.textarea.lines()[0].trim().to_string();
        if text.is_empty() {
            return;
        }
        self.events.push(Event::new(Sender::User, text));

        let history = to_messages(&self.events);
        let reply = self
            .chat
            .reply(&history)
            .unwrap_or_else(|_| "Sorry, something went wrong.".to_string());

        self.events.push(Event::new(Sender::Assistant, reply));
        self.textarea.clear();
    }
}

/// Converts the transcript into the provider-agnostic form the llm module
/// expects.
fn to_messages(events: &[Event]) -> Vec<Message> {
    events
        .iter()
        .map(|e| Message {
            role: match e.sender {
                Sender::User => Role::User,
                Sender::Assistant => Role::Assistant,
            },
            content: e.content.clone(),
        })
        .collect()
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message and press Enter...");
    textarea.set_cursor_line_style(Style::default());
    textarea
}
