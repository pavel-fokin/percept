use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

use crate::event::{stub_assistant_reply, Event, Sender};

pub struct App<'a> {
    pub events: Vec<Event>,
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
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
        }
    }

    /// Appends the pending input as a user event plus an immediate stub
    /// assistant reply, then clears the input.
    pub fn submit(&mut self) {
        let text = self.textarea.lines()[0].trim().to_string();
        if text.is_empty() {
            return;
        }
        let reply = stub_assistant_reply(&text);
        self.events.push(Event::new(Sender::User, text));
        self.events.push(Event::new(Sender::Assistant, reply));
        self.textarea.clear();
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message and press Enter...");
    textarea.set_cursor_line_style(Style::default());
    textarea
}
