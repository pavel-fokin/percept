use ratatui::style::Style;
use ratatui::text::Line;

use super::ui::{clip, turn_lines};

/// A collapsed thought's lines: a `✻` gutter and a dimmed, clipped
/// preview - same convention tool activity uses, so a committed
/// thought reads as context rather than dialogue.
pub fn collapsed_lines(
    content: &str,
    marker_style: Style,
    body_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    turn_lines("✻", marker_style, body_style, &clip(content), width)
}
