use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

mod ui;
mod update;

pub use ui::draw;
pub use update::{handle_key, handle_stream};

use crate::app::AppService;
use crate::percept::Chunk;

/// Adapts the reply stream onto tokio's mpsc channel, so the main
/// select! loop can drive it alongside terminal events. Local to tui -
/// `app` never sees this type.
pub enum StreamEvent {
    Chunk(Chunk),
    /// The turn is over. `Some` carries why it broke, in the provider's
    /// own words, so "ollama isn't running" and "the model isn't
    /// pulled" read differently. Never a Chunk - a chunk becomes a
    /// committed model turn, and the log records what the model said,
    /// not what failed while asking.
    Ended(Option<String>),
}

/// Chat is tui's own state - textarea, styling - plus whatever fulfills
/// AppService. It renders and forwards input; it holds no chat logic.
pub struct Chat<'a> {
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
    pub thought_style: Style,
    pub error_style: Style,
    /// Chrome, not content: the input's border and the status row.
    pub hint_style: Style,
    /// Which spinner frame the status row shows. Advanced by `tick`
    /// while a turn streams, so a wait with no tokens yet still moves.
    pub spinner: usize,
    pub app: Box<dyn AppService>,
    /// Why the last reply broke, shown until the next submit. Transient
    /// tui state - it never reaches the log.
    pub error: Option<String>,
}

impl<'a> Chat<'a> {
    pub fn new(app: Box<dyn AppService>) -> Self {
        Self {
            textarea: new_textarea(),
            user_style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            assistant_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            thought_style: Style::default().fg(Color::DarkGray),
            error_style: Style::default().fg(Color::Red),
            hint_style: Style::default().fg(Color::DarkGray),
            spinner: 0,
            app,
            error: None,
        }
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Send a message…");
    textarea.set_cursor_line_style(Style::default());
    textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
    textarea
}
