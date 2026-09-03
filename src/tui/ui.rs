use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph};
use ratatui::Frame;

use super::Chat;
use crate::percept::{Actor, Event, EventId, EventKind, Payload};

/// One marker plus a space. Every wrapped line of a turn indents past
/// it, so the gutter stays a column of markers and nothing else.
const GUTTER: &str = "  ";

/// Braille frames, advanced by `Chat::tick` while a turn streams.
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Longest tool argument or result shown in the transcript. Tool
/// activity is context, not dialogue - a peek is enough.
const TOOL_PREVIEW: usize = 200;

/// Rows the status may take. A wrapped error grows it; the cap keeps a
/// long one from squeezing the transcript off the screen.
const STATUS_MAX: usize = 4;

pub fn draw(frame: &mut Frame, chat: &mut Chat) {
    // One blank column each side, so text never touches the edge.
    let area = frame.area().inner(Margin::new(1, 0));
    let width = area.width.max(1) as usize;

    let status = status(chat, width);
    let status_height = status.len().min(STATUS_MAX) as u16;
    let input_height = input_height(chat, area.height, status_height + 1);
    let [transcript_area, input_area, status_area, model_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(input_height),
        Constraint::Length(status_height),
        Constraint::Length(1),
    ])
    .areas(area);

    let (text, scroll_limit, thought_rows) = transcript(chat, transcript_area);
    chat.update_scroll_metrics(scroll_limit, transcript_area.height);
    chat.update_thought_rows(transcript_area.y, thought_rows);
    frame.render_widget(
        Paragraph::new(text).scroll((chat.scroll_offset, 0)),
        transcript_area,
    );
    draw_input(frame, chat, input_area);
    frame.render_widget(Paragraph::new(status), status_area);
    frame.render_widget(Paragraph::new(model_status(chat)), model_area);
}

/// The box has one row per entered line plus its top and bottom border.
/// Keep one row for the transcript when the terminal is short.
/// `below_height` is the status row plus the model row beneath it.
fn input_height(chat: &Chat, total_height: u16, below_height: u16) -> u16 {
    let wanted = (chat.textarea.lines().len() as u16).saturating_add(2);
    let available = total_height.saturating_sub(below_height).saturating_sub(1);
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

/// Wrapped, styled transcript lines, the furthest valid offset, and
/// which committed thought (if any) each line belongs to - so a click
/// can be mapped back to the event it landed on.
fn transcript(chat: &Chat, area: Rect) -> (Text<'static>, u16, Vec<Option<EventId>>) {
    let width = area.width.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut ids: Vec<Option<EventId>> = Vec::new();
    for event in chat.app.events() {
        let id = matches!(event.payload(), Payload::ThoughtRecorded { .. }).then(|| event.id());
        push_turn(&mut lines, &mut ids, id, event_lines(chat, event, width));
    }
    // Full and dimmed while it streams; once committed it renders
    // collapsed instead, in `event_lines`. Never clickable, so no id.
    if let Some(thought) = chat.app.pending_thought() {
        push_turn(
            &mut lines,
            &mut ids,
            None,
            turn_lines("✻", chat.thought_style, chat.thought_style, thought, width),
        );
    }
    if let Some(reply) = chat.app.pending_reply() {
        push_turn(
            &mut lines,
            &mut ids,
            None,
            actor_lines(chat, Actor::Model, reply, width),
        );
    }

    let limit = lines.len().saturating_sub(area.height as usize);
    (Text::from(lines), limit.try_into().unwrap_or(u16::MAX), ids)
}

/// Adds one turn's lines, separated from the turn above by a blank
/// line. Turns run together without it, which is what makes a long
/// transcript hard to scan. `id` is the event the turn's lines - and
/// the blank separator, if any - map back to, for `ids`.
fn push_turn(
    lines: &mut Vec<Line<'static>>,
    ids: &mut Vec<Option<EventId>>,
    id: Option<EventId>,
    turn: Vec<Line<'static>>,
) {
    if turn.is_empty() {
        return;
    }
    if !lines.is_empty() {
        lines.push(Line::default());
        ids.push(None);
    }
    ids.extend(std::iter::repeat_n(id, turn.len()));
    lines.extend(turn);
}

fn event_lines(chat: &Chat, event: &Event, width: usize) -> Vec<Line<'static>> {
    match event.payload() {
        Payload::MessageReceived { content } => actor_lines(chat, event.actor(), content, width),
        // Tool activity shows dimmed, so the reader can see what the
        // model looked up.
        Payload::ToolCalled {
            tool, arguments, ..
        } => tool_lines(chat, &format!("{tool} {}", clip(arguments)), width),
        Payload::ToolResulted { content, .. } => tool_lines(chat, &clip(content), width),
        Payload::ThoughtRecorded { content } => {
            let lines = if chat.is_thought_expanded(event.id()) {
                super::thought::expanded_lines
            } else {
                super::thought::collapsed_lines
            };
            lines(content, chat.thought_style, chat.thought_style, width)
        }
        // Bookkeeping about a round trip, not something to show.
        Payload::ModelCalled(..) => Vec::new(),
        // A map change shows dimmed too - it's context the model built,
        // not dialogue.
        Payload::NodeAdded {
            map, kind, name, ..
        } => tool_lines(chat, &format!("{map}: added {kind} {name:?}"), width),
        Payload::NodeRemoved {
            map, node, reason, ..
        } => tool_lines(
            chat,
            &format!("{map}: removed node {} - {reason}", node.as_uuid()),
            width,
        ),
        Payload::EdgeAdded {
            map,
            kind,
            from,
            to,
            ..
        }
        | Payload::EdgeRemoved {
            map,
            kind,
            from,
            to,
            ..
        } => {
            let verb = match event.kind() {
                EventKind::EdgeAdded => "added",
                _ => "removed",
            };
            tool_lines(
                chat,
                &format!(
                    "{map}: {verb} edge {kind} {} \u{2192} {}",
                    from.as_uuid(),
                    to.as_uuid()
                ),
                width,
            )
        }
    }
}

/// A tool call or its result: a `⚒` gutter and dimmed body, so it
/// reads as context beside the dialogue.
fn tool_lines(chat: &Chat, body: &str, width: usize) -> Vec<Line<'static>> {
    turn_lines("⚒", chat.hint_style, chat.thought_style, body, width)
}

/// One preview string: whitespace flattened so a multi-line result
/// stays compact, then cut to `TOOL_PREVIEW` characters on a boundary.
pub(super) fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(TOOL_PREVIEW) {
        Some((idx, _)) => format!("{}…", &flat[..idx]),
        None => flat,
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
pub(super) fn turn_lines(
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

/// The row under the status row: the model's name and the last round
/// trip's input tokens against its context window, when the window is
/// known. No round trip yet - this session's or an earlier one's, from
/// the log it opened on - reads as zero, not as nothing.
fn model_status(chat: &Chat) -> Line<'static> {
    let name = chat.app.model_name();
    let used = chat.app.last_usage().map_or(0, |usage| usage.input_tokens);
    let body = match chat.app.context_window() {
        Some(window) => format!(
            "{name} · {} / {} tokens",
            thousands(used),
            thousands(window as u64)
        ),
        None => format!("{name} · {} tokens", thousands(used)),
    };
    Line::from(Span::styled(body, chat.hint_style))
}

/// `12345` as `"12,345"` - ratatui has no number formatting of its own.
fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
