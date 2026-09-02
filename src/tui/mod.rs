use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

mod ui;
mod update;

pub use ui::draw;
pub use update::{handle_key, handle_mouse, handle_stream};

use crate::app::AppService;
use crate::percept::Chunk;

/// Adapts the reply stream onto tokio's mpsc channel, so the main
/// select! loop can drive it alongside terminal events. Local to tui -
/// `app` never sees this type.
pub enum StreamEvent {
    Chunk(Chunk),
    /// A tool finished off-thread; the string is what to feed back. On
    /// its own event so the blocking `Tool::run` never sits on the main
    /// loop.
    ToolResult(String),
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
    /// The first visible transcript line. It stays put while the user
    /// reads history, even as a reply adds new lines below it.
    pub scroll_offset: u16,
    pub scroll_limit: u16,
    pub page_height: u16,
    pub follows_transcript: bool,
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
            scroll_offset: 0,
            scroll_limit: 0,
            page_height: 1,
            follows_transcript: true,
            app,
            error: None,
        }
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }

    pub fn update_scroll_metrics(&mut self, limit: u16, page_height: u16) {
        self.scroll_limit = limit;
        self.page_height = page_height.max(1);
        if self.follows_transcript {
            self.scroll_offset = limit;
        } else {
            self.scroll_offset = self.scroll_offset.min(limit);
        }
    }

    pub fn scroll_up(&mut self, lines: u16) {
        self.follows_transcript = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self
            .scroll_offset
            .saturating_add(lines)
            .min(self.scroll_limit);
        self.follows_transcript = self.scroll_offset == self.scroll_limit;
    }

    pub fn scroll_to_top(&mut self) {
        self.follows_transcript = false;
        self.scroll_offset = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.follows_transcript = true;
        self.scroll_offset = self.scroll_limit;
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Send a message…");
    textarea.set_cursor_line_style(Style::default());
    textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
    textarea
}
