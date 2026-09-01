use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

mod ui;
mod update;

pub use ui::draw;
pub use update::handle_key;

use crate::app::AppService;

/// Adapts the reply stream onto tokio's mpsc channel, so the main
/// select! loop can drive it alongside terminal events. Local to tui -
/// `app` never sees this type.
pub enum StreamEvent {
    Chunk(String),
    Done,
    /// The reply broke. Carries the provider's own words, so "ollama
    /// isn't running" and "the model isn't pulled" read differently.
    /// Never a Chunk - a chunk becomes a committed model turn, and the
    /// log records what the model said, not what failed while asking.
    Failed(String),
}

/// Chat is tui's own state - textarea, styling - plus whatever fulfills
/// AppService. It renders and forwards input; it holds no chat logic.
pub struct Chat<'a> {
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
    pub error_style: Style,
    pub app: Box<dyn AppService>,
    /// Why the last reply broke, shown until the next submit. Transient
    /// tui state - it never reaches the log.
    pub error: Option<String>,
    /// True from submit until the reply ends. A second submit before
    /// then would overwrite the first reply's cause and merge both
    /// replies into one event, and an append-only log keeps the damage.
    pub replying: bool,
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
            error_style: Style::default().fg(Color::Red),
            app,
            error: None,
            replying: false,
        }
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message and press Enter...");
    textarea.set_cursor_line_style(Style::default());
    textarea
}
