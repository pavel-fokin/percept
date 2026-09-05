use std::time::Instant;

use ratatui::style::{Color, Modifier, Style};
use ratatui_textarea::TextArea;

mod commands;
mod thought;
mod ui;
mod update;

pub use ui::draw;
pub use update::{handle_key, handle_mouse, handle_stream};

use crate::app::AppService;
use crate::percept::{Chunk, EventId, ModelDescriptor, ToolOutput};

/// Adapts the reply stream onto tokio's mpsc channel, so the main
/// select! loop can drive it alongside terminal events. Local to tui -
/// `app` never sees this type.
pub enum StreamEvent {
    Chunk(Chunk),
    /// A tool finished off-thread; this is what to feed back.
    /// `App::finish_tool` takes it. On its own event so the blocking
    /// `Tool::run` never sits on the main loop.
    ToolResult(ToolOutput),
    /// The turn is over. `Some` carries why it broke, in the provider's
    /// own words, so "ollama isn't running" and "the model isn't
    /// pulled" read differently. Never a Chunk - a chunk becomes a
    /// committed model turn, and the log records what the model said,
    /// not what failed while asking.
    Ended(Option<String>),
    /// `/models`'s fetch landed. On its own event because `available_models`
    /// queries every provider and can't block the main loop. Carries the
    /// token of the menu the fetch was started for, so a fetch from a
    /// popup the user already closed and reopened lands on neither.
    ModelsListed(u32, Vec<ModelDescriptor>),
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
    /// When the turn now streaming was submitted. Backs the "Thinking…"
    /// seconds counter - shown only until the reply itself starts
    /// streaming, not the thought that may precede it.
    pub thinking_started: Option<Instant>,
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
    /// Open while the `/models` popup shows - `None` the rest of the
    /// time, when keys reach the textarea as usual.
    pub models_menu: Option<ModelsMenu>,
    /// Commands whose name starts with the input line's prefix, while
    /// it starts with `/`. Empty otherwise, so the anchored list above
    /// the input reserves no space. Recomputed from the textarea after
    /// every keystroke that reaches it - not yet wired to selection.
    pub command_suggestions: Vec<&'static commands::Command>,
    /// Counted up each time `/models` opens, so the `ModelsMenu` it
    /// opens can be told apart from one closed and reopened since - see
    /// `ModelsMenu::token`.
    next_models_token: u32,
    /// Committed thoughts a click has expanded. Small and short-lived,
    /// so a linear scan beats giving `EventId` a `Hash` impl just for
    /// this.
    expanded_thoughts: Vec<EventId>,
    /// Last frame's transcript rows, one entry per line: the thought
    /// that line belongs to, or `None`. Lets a click map a screen row
    /// back to the event it landed on.
    thought_rows: Vec<Option<EventId>>,
    /// The transcript area's top row, so `thought_at` can turn a
    /// terminal-absolute mouse row into an offset into `thought_rows`.
    thought_rows_top: u16,
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
            thinking_started: None,
            scroll_offset: 0,
            scroll_limit: 0,
            page_height: 1,
            follows_transcript: true,
            app,
            error: None,
            models_menu: None,
            command_suggestions: Vec::new(),
            next_models_token: 0,
            expanded_thoughts: Vec::new(),
            thought_rows: Vec::new(),
            thought_rows_top: 0,
        }
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }

    /// Refilters `command_suggestions` from the textarea's current
    /// line: every `Command` whose name starts with it, while the
    /// trimmed line itself starts with `/`. Empty otherwise, so a line
    /// that no longer starts with `/`, or matches nothing, hides the
    /// list.
    pub fn recompute_command_suggestions(&mut self) {
        let text = self.textarea.lines().join("\n");
        let prefix = text.trim();
        self.command_suggestions = if prefix.starts_with('/') {
            commands::COMMANDS
                .iter()
                .filter(|command| command.name.starts_with(prefix))
                .collect()
        } else {
            Vec::new()
        };
    }

    /// A token no earlier `/models` open holds, for a menu about to
    /// open.
    pub fn new_models_token(&mut self) -> u32 {
        self.next_models_token = self.next_models_token.wrapping_add(1);
        self.next_models_token
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

    pub fn update_thought_rows(&mut self, top: u16, rows: Vec<Option<EventId>>) {
        self.thought_rows_top = top;
        self.thought_rows = rows;
    }

    pub fn is_thought_expanded(&self, id: EventId) -> bool {
        self.expanded_thoughts.contains(&id)
    }

    pub fn toggle_thought(&mut self, id: EventId) {
        if self.expanded_thoughts.contains(&id) {
            self.expanded_thoughts.retain(|existing| *existing != id);
        } else {
            self.expanded_thoughts.push(id);
        }
    }

    /// The thought, if any, whose transcript line a click at `row`
    /// landed on. `row` is terminal-absolute, so it's first brought
    /// back to a transcript-relative row, then offset by how far the
    /// view has scrolled.
    pub fn thought_at(&self, row: u16) -> Option<EventId> {
        let content_row = self.scroll_offset + row.saturating_sub(self.thought_rows_top);
        self.thought_rows
            .get(content_row as usize)
            .copied()
            .flatten()
    }
}

/// The `/models` popup's state: the fetched list, once it lands, and
/// which row is selected. `descriptors` is `None` while the fetch is
/// still in flight, so a still-loading popup reads differently from
/// one that loaded and found nothing.
pub struct ModelsMenu {
    descriptors: Option<Vec<ModelDescriptor>>,
    selected: usize,
    /// Ties this menu to the fetch `open_models_menu` started for it -
    /// see `StreamEvent::ModelsListed`.
    token: u32,
}

impl ModelsMenu {
    pub fn loading(token: u32) -> Self {
        Self {
            descriptors: None,
            selected: 0,
            token,
        }
    }

    pub fn token(&self) -> u32 {
        self.token
    }

    pub fn populate(&mut self, descriptors: Vec<ModelDescriptor>) {
        self.selected = self.selected.min(descriptors.len().saturating_sub(1));
        self.descriptors = Some(descriptors);
    }

    pub fn descriptors(&self) -> Option<&[ModelDescriptor]> {
        self.descriptors.as_deref()
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn selected(&self) -> Option<&ModelDescriptor> {
        self.descriptors()?.get(self.selected)
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn move_down(&mut self) {
        let len = self.descriptors().map_or(0, <[ModelDescriptor]>::len);
        if self.selected + 1 < len {
            self.selected += 1;
        }
    }
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Send a message…");
    textarea.set_cursor_line_style(Style::default());
    textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
    textarea
}

#[cfg(test)]
mod tests;
