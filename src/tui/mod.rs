use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

mod ui;
mod update;

pub use ui::draw;
pub use update::handle_key;

use crate::app::AppService;

/// What the reply channel carries: a piece of the streamed reply, or a
/// signal that the stream is finished. Local to tui - the boundary
/// where the thunk's callback-based streaming gets adapted onto tokio's
/// mpsc channel; `app` never sees this type.
pub enum StreamEvent {
    Chunk(String),
    Done,
}

/// Chat is tui's own state - textarea, styling - plus whatever fulfills
/// AppService. It renders and forwards input; it holds no chat logic.
pub struct Chat<'a> {
    pub textarea: TextArea<'a>,
    pub user_style: Style,
    pub assistant_style: Style,
    pub conversation: Box<dyn AppService>,
}

impl<'a> Chat<'a> {
    pub fn new(conversation: Box<dyn AppService>) -> Self {
        Self {
            textarea: new_textarea(),
            user_style: Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            assistant_style: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            conversation,
        }
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Type a message and press Enter...");
    textarea.set_cursor_line_style(Style::default());
    textarea
}
