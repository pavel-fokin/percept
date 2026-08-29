use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::message::{ChatMessage, Sender};

pub fn draw(frame: &mut Frame, app: &App) {
    let [transcript_area, input_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    let (text, offset) = transcript(app, transcript_area);
    frame.render_widget(Paragraph::new(text).scroll((offset, 0)), transcript_area);
    frame.render_widget(&app.textarea, input_area);
}

/// Wrapped, styled transcript lines plus the scroll offset that pins the
/// view to the bottom. There's no persistent scroll state to update -
/// ratatui redraws from scratch every frame, so this just recomputes it.
fn transcript(app: &App, area: Rect) -> (Text<'static>, u16) {
    let width = area.width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    for msg in &app.messages {
        lines.extend(message_lines(app, msg, width));
    }

    let total = lines.len() as u16;
    let offset = total.saturating_sub(area.height);
    (Text::from(lines), offset)
}

fn message_lines(app: &App, msg: &ChatMessage, width: usize) -> Vec<Line<'static>> {
    let (style, prefix) = match msg.from {
        Sender::User => (app.user_style, "You: "),
        Sender::Assistant => (app.assistant_style, "Assistant: "),
    };
    let full_line = format!("{prefix}{}", msg.text);

    textwrap::wrap(&full_line, width)
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == 0 {
                let (p, rest) = w.split_at(prefix.len().min(w.len()));
                Line::from(vec![Span::styled(p.to_string(), style), Span::raw(rest.to_string())])
            } else {
                Line::from(w.to_string())
            }
        })
        .collect()
}
