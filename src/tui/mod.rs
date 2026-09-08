use std::sync::Arc;
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
use crate::core::EventId;
use crate::harness::{Chunk, ModelDescriptor, Tool, ToolOutput};

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

/// One row of the anchored suggestion list: the text that replaces the
/// input line when it is accepted, and the help shown beside it. A
/// command suggestion carries the command name; a `/effort` value
/// suggestion carries the whole `/effort <level>` line.
pub struct Suggestion {
    pub value: String,
    pub description: String,
}

impl Suggestion {
    fn command(command: &commands::Command) -> Self {
        Self {
            value: command.name.to_string(),
            description: command.description.to_string(),
        }
    }
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
    /// What the last command did, shown until the next submit - the
    /// quiet counterpart of `error`. Transient like it.
    pub notice: Option<String>,
    /// Open while the `/models` popup shows - `None` the rest of the
    /// time, when keys reach the textarea as usual.
    pub models_menu: Option<ModelsMenu>,
    /// A tool call waiting on the user's yes or no - `App` returned
    /// `ToolStep::Ask` and the turn is paused until a key answers.
    /// Keys go to it, not the textarea, while it is `Some`.
    pub approval: Option<Approval>,
    /// Rows for the anchored list above the input, while the line
    /// starts with `/`: commands whose name matches the prefix, or the
    /// reasoning levels the active model allows once the line is
    /// `/effort`. Empty otherwise, so the list reserves no space.
    /// Recomputed from the textarea after every keystroke that reaches
    /// it.
    pub command_suggestions: Vec<Suggestion>,
    /// Which row of `command_suggestions` is highlighted. Reset to 0
    /// whenever the list is recomputed, since a narrower or wider match
    /// makes an old row meaningless.
    pub command_selected: usize,
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
            notice: None,
            models_menu: None,
            approval: None,
            command_suggestions: Vec::new(),
            command_selected: 0,
            next_models_token: 0,
            expanded_thoughts: Vec::new(),
            thought_rows: Vec::new(),
            thought_rows_top: 0,
        }
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
    }

    /// The textarea's lines joined back into one string - a slash
    /// command is always single-line, but the textarea itself doesn't
    /// know that.
    pub fn current_text(&self) -> String {
        self.textarea.lines().join("\n")
    }

    /// Consumes the input line: clears the textarea and its
    /// suggestions, and the last command's error and notice with them,
    /// since what they were about is over.
    pub fn take_input(&mut self) {
        self.textarea.clear();
        self.recompute_command_suggestions();
        self.error = None;
        self.notice = None;
    }

    /// Refilters `command_suggestions` from the textarea's content. On
    /// the `/effort` line - the command typed, then optionally a
    /// partial level - it lists the reasoning levels the active model
    /// allows that the partial still matches. Otherwise, while the
    /// trimmed content starts with `/`, it lists every `Command` whose
    /// name starts with it. Empty otherwise, so content that no longer
    /// starts with `/`, or matches nothing, hides the list. The
    /// highlighted row resets to the top - a narrower or wider match
    /// makes the old row meaningless.
    pub fn recompute_command_suggestions(&mut self) {
        let text = self.current_text();
        let prefix = text.trim();
        self.command_suggestions = if let Some(partial) = effort_partial(&text) {
            self.app
                .supported_reasoning_efforts()
                .iter()
                .map(|effort| effort.to_string())
                .filter(|level| level.starts_with(partial))
                .map(|level| Suggestion {
                    value: format!("{} {level}", commands::EFFORT),
                    description: "reasoning level".to_string(),
                })
                .collect()
        } else if prefix.starts_with('/') {
            commands::COMMANDS
                .iter()
                .filter(|command| command.name.starts_with(prefix))
                .map(Suggestion::command)
                .collect()
        } else {
            Vec::new()
        };
        self.command_selected = 0;
    }

    /// Moves the highlighted suggestion up one row, clamped at the
    /// first - same style as `ModelsMenu::move_up`.
    pub fn move_command_selection_up(&mut self) {
        self.command_selected = self.command_selected.saturating_sub(1);
    }

    /// Moves the highlighted suggestion down one row, clamped at the
    /// last - same style as `ModelsMenu::move_down`.
    pub fn move_command_selection_down(&mut self) {
        if self.command_selected + 1 < self.command_suggestions.len() {
            self.command_selected += 1;
        }
    }

    /// Replaces the textarea's line with the highlighted suggestion's
    /// value, then hides the dropdown. The user just accepted their
    /// choice; showing it again a frame later, still matching itself as
    /// a prefix, would have nothing left to narrow.
    pub fn accept_command_suggestion(&mut self) {
        if let Some(suggestion) = self.command_suggestions.get(self.command_selected) {
            let value = suggestion.value.clone();
            self.textarea.clear();
            self.textarea.insert_str(value);
        }
        self.close_command_suggestions();
    }

    /// Hides the command dropdown and resets which row was highlighted,
    /// so it doesn't carry over into the next time it opens.
    pub fn close_command_suggestions(&mut self) {
        self.command_suggestions = Vec::new();
        self.command_selected = 0;
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

/// One tool call the policy put to the user, held until they answer.
pub struct Approval {
    pub tool: Arc<dyn Tool>,
    pub arguments: String,
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

/// The `/effort` argument on the trimmed input line: `Some("")` for
/// the bare command, `Some(token)` for the one token after it. `None`
/// when the line is not `/effort`, or carries more than one token -
/// so the grammar of the command lives here, for both the suggestion
/// filter and the command dispatch in `update`.
fn effort_partial(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix(commands::EFFORT)?;
    if rest.is_empty() {
        return Some("");
    }
    let arg = rest.strip_prefix(char::is_whitespace)?.trim_start();
    (!arg.contains(char::is_whitespace)).then_some(arg)
}

fn new_textarea<'a>() -> TextArea<'a> {
    let mut textarea = TextArea::default();
    textarea.set_placeholder_text("Send a message…");
    textarea.set_cursor_line_style(Style::default());
    textarea.set_placeholder_style(Style::default().fg(Color::DarkGray));
    textarea
}

/// Types `text` into `chat`'s textarea one character at a time and
/// refilters its command suggestions, the way `handle_key`'s catch-all
/// arm does for real input. Shared by `tui::tests` and
/// `tui::update::tests`.
#[cfg(test)]
fn type_str(chat: &mut Chat, text: &str) {
    for ch in text.chars() {
        chat.textarea.insert_char(ch);
    }
    chat.recompute_command_suggestions();
}

#[cfg(test)]
mod tests;
