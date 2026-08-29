use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

use crate::message::{stub_assistant_reply, ChatMessage, Sender};

pub struct App<'a> {
    pub messages: Vec<ChatMessage>,
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
}

impl<'a> App<'a> {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            textarea: new_textarea(),
            user_style: Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            assistant_style: Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        }
    }

    /// Appends the pending input as a user message plus an immediate stub
    /// assistant reply, then clears the input.
    pub fn submit(&mut self) {
        let text = self.textarea.lines()[0].trim().to_string();
        if text.is_empty() {
            return;
        }
        let reply = stub_assistant_reply(&text);
        self.messages.push(ChatMessage { from: Sender::User, text });
        self.messages.push(ChatMessage { from: Sender::Assistant, text: reply });
        self.textarea.clear();
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message and press Enter...");
    textarea.set_cursor_line_style(Style::default());
    textarea
}
