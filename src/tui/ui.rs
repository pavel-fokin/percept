use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use super::Chat;
use crate::percept::{Actor, Event, Payload};

/// One marker plus a space. Every wrapped line of a turn indents past
/// it, so the gutter stays a column of markers and nothing else.
const GUTTER: &str = "  ";

/// Braille frames, advanced by `Chat::tick` while a turn streams.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Rows the status may take. A wrapped error grows it; the cap keeps a
/// long one from squeezing the transcript off the screen.
const STATUS_MAX: usize = 4;

pub fn draw(frame: &mut Frame, chat: &mut Chat) {
    // One blank column each side, so text never touches the edge.
    let area = frame.area().inner(Margin::new(1, 0));
    let width = area.width.max(1) as usize;

    let status = status(chat, width);
    let input_height = input_height(chat, area.height, status.len() as u16);
    let [transcript_area, input_area, status_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(input_height),
        Constraint::Length(status.len().min(STATUS_MAX) as u16),
    ])
    .areas(area);

    let (text, scroll_limit) = transcript(chat, transcript_area);
    chat.update_scroll_metrics(scroll_limit, transcript_area.height);
    frame.render_widget(
        Paragraph::new(text).scroll((chat.scroll_offset, 0)),
        transcript_area,
    );
    draw_input(frame, chat, input_area);
    frame.render_widget(Paragraph::new(status), status_area);
}

/// The box has one row per entered line plus its top and bottom border.
/// Keep one row for the transcript when the terminal is short.
fn input_height(chat: &Chat, total_height: u16, status_height: u16) -> u16 {
    let wanted = (chat.textarea.lines().len() as u16).saturating_add(2);
    let available = total_height.saturating_sub(status_height).saturating_sub(1);
    wanted.min(available).max(1)
}

/// The input, framed and prompted, so it reads as somewhere to type
/// rather than the last line of the transcript.
fn draw_input(frame: &mut Frame, chat: &Chat, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(chat.hint_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [prompt_area, entry_area] =
        Layout::horizontal([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
    frame.render_widget(
        Paragraph::new(Span::styled("> ", chat.user_style)),
        prompt_area,
    );
    frame.render_widget(&chat.textarea, entry_area);
}

/// Wrapped, styled transcript lines plus the furthest valid offset.
fn transcript(chat: &Chat, area: Rect) -> (Text<'static>, u16) {
    let width = area.width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for event in chat.app.events() {
        push_turn(&mut lines, event_lines(chat, event, width));
    }
    // Dimmed, and gone once the thought commits - a recorded thought
    // isn't dialogue, so a reloaded transcript never shows it.
    if let Some(thought) = chat.app.pending_thought() {
        push_turn(
            &mut lines,
            turn_lines("✻", chat.thought_style, chat.thought_style, thought, width),
        );
    }
    if let Some(reply) = chat.app.pending_reply() {
        push_turn(&mut lines, actor_lines(chat, Actor::Model, reply, width));
    }

    let limit = lines.len().saturating_sub(area.height as usize);
    (Text::from(lines), limit.try_into().unwrap_or(u16::MAX))
}

/// Adds one turn's lines, separated from the turn above by a blank
/// line. Turns run together without it, which is what makes a long
/// transcript hard to scan.
fn push_turn(lines: &mut Vec<Line<'static>>, turn: Vec<Line<'static>>) {
    if turn.is_empty() {
        return;
    }
    if !lines.is_empty() {
        lines.push(Line::default());
    }
    lines.extend(turn);
}

fn event_lines(chat: &Chat, event: &Event, width: usize) -> Vec<Line<'static>> {
    match event.payload() {
        Payload::MessageReceived { content } => actor_lines(chat, event.actor(), content, width),
        // Not dialogue - tui renders the transcript, not tool activity
        // or a thought already shown while it streamed.
        Payload::ToolUsed { .. } => Vec::new(),
        Payload::ThoughtRecorded { .. } => Vec::new(),
    }
}

fn actor_lines(chat: &Chat, actor: Actor, content: &str, width: usize) -> Vec<Line<'static>> {
    let (marker, style) = match actor {
        Actor::User => (">", chat.user_style),
        Actor::Model => ("⏺", chat.assistant_style),
        Actor::System => ("⏺", chat.hint_style),
    };
    turn_lines(marker, style, Style::default(), content, width)
}

/// One turn: its marker in the gutter, its text wrapped and hanging
/// under itself.
fn turn_lines(
    marker: &str,
    marker_style: Style,
    body_style: Style,
    content: &str,
    width: usize,
) -> Vec<Line<'static>> {
    let body_width = width.saturating_sub(GUTTER.len()).max(1);
    textwrap::wrap(content, body_width)
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let gutter = if i == 0 {
                Span::styled(format!("{marker} "), marker_style)
            } else {
                Span::raw(GUTTER)
            };
            Line::from(vec![gutter, Span::styled(text.to_string(), body_style)])
        })
        .collect()
}

/// The row under the input: what went wrong, what the model is doing,
/// or which keys to press. Apart from the transcript because none of
/// it reaches the log.
fn status(chat: &Chat, width: usize) -> Vec<Line<'static>> {
    if let Some(error) = &chat.error {
        // Wrapped rather than truncated - a provider's own words are
        // the whole reason the error is shown.
        return turn_lines("!", chat.error_style, chat.error_style, error, width);
    }
    if chat.app.is_replying() {
        let frame = SPINNER[chat.spinner % SPINNER.len()];
        // A first token can be minutes away while ollama loads a
        // model, so the label says which half of the wait this is.
        let label = match chat.app.pending_reply() {
            Some(_) => "Responding…",
            None => "Thinking…",
        };
        return vec![Line::from(vec![
            Span::styled(format!("{frame} "), chat.assistant_style),
            Span::styled(label, chat.hint_style),
        ])];
    }
    if !chat.follows_transcript {
        return vec![Line::from(Span::styled(
            "PgUp/PgDn scroll · End latest",
            chat.hint_style,
        ))];
    }
    vec![Line::from(Span::styled(
        "Enter send · Ctrl+J newline · PgUp/PgDn scroll · Esc quit",
        chat.hint_style,
    ))]
}
