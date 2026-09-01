use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::Chat;
use crate::percept::{Actor, Event, Payload};

pub fn draw(frame: &mut Frame, chat: &Chat) {
    let [transcript_area, input_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let (text, offset) = transcript(chat, transcript_area);
    frame.render_widget(Paragraph::new(text).scroll((offset, 0)), transcript_area);
    frame.render_widget(&chat.textarea, input_area);
}

/// Wrapped, styled transcript lines plus the scroll offset that pins the
/// view to the bottom. There's no persistent scroll state to update -
/// ratatui redraws from scratch every frame, so this just recomputes it.
fn transcript(chat: &Chat, area: Rect) -> (Text<'static>, u16) {
    let width = area.width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for event in chat.app.events() {
        lines.extend(event_lines(chat, event, width));
    }
    // Dimmed, and gone once the thought commits - a recorded thought
    // isn't dialogue, so a reloaded transcript never shows it.
    if let Some(thought) = chat.app.pending_thought() {
        lines.extend(wrapped(thought, chat.thought_style, width));
    }
    if let Some(reply) = chat.app.pending_reply() {
        lines.extend(styled_lines(chat, Actor::Model, reply, width));
    }

    // Apart from the transcript because it isn't part of it - nothing
    // here reaches the log.
    if let Some(error) = &chat.error {
        lines.extend(wrapped(&format!("! {error}"), chat.error_style, width));
    }

    let total = lines.len() as u16;
    let offset = total.saturating_sub(area.height);
    (Text::from(lines), offset)
}

/// Wrapped lines in one style, for text that isn't dialogue: it takes
/// no `Assistant: ` prefix and no per-span styling.
fn wrapped(text: &str, style: Style, width: usize) -> Vec<Line<'static>> {
    textwrap::wrap(text, width)
        .iter()
        .map(|w| Line::from(Span::styled(w.to_string(), style)))
        .collect()
}

fn event_lines(chat: &Chat, event: &Event, width: usize) -> Vec<Line<'static>> {
    match event.payload() {
        Payload::MessageReceived { content } => styled_lines(chat, event.actor(), content, width),
        // Not dialogue - tui renders the transcript, not tool activity
        // or a thought already shown while it streamed.
        Payload::ToolUsed { .. } => Vec::new(),
        Payload::ThoughtRecorded { .. } => Vec::new(),
    }
}

fn styled_lines(chat: &Chat, actor: Actor, content: &str, width: usize) -> Vec<Line<'static>> {
    let (style, prefix) = match actor {
        Actor::User => (chat.user_style, "You: "),
        Actor::Model => (chat.assistant_style, "Assistant: "),
        Actor::System => (chat.assistant_style, "System: "),
    };
    let full_line = format!("{prefix}{content}");

    textwrap::wrap(&full_line, width)
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                let (p, rest) = w.split_at(prefix.len().min(w.len()));
                Line::from(vec![
                    Span::styled(p.to_string(), style),
                    Span::raw(rest.to_string()),
                ])
            } else {
                Line::from(w.to_string())
            }
        })
        .collect()
}
