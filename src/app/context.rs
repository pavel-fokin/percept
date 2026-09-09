use std::fmt;

use crate::app::MapShape;
use crate::core::{Actor, Event, EventId, EventKind, Map, Schemas, Scope, PREVIEW_CHARS};
use crate::shared::Timestamp;

/// A map's size and age in one clause, so the model can tell whether
/// opening it is worth a call: what a catalogue says about a map it
/// does not show.
fn catalogue_line(map: &Map) -> String {
    match map.last_changed() {
        Some(at) => format!(
            "It holds {} nodes and {} edges, last changed {at}",
            map.nodes().len(),
            map.edges().len()
        ),
        None => "It holds nothing yet".to_string(),
    }
}

/// `MapShape::Headlines`'s body: the headline nodes as `Map`'s
/// `Display` formats a node line, without properties - a reader
/// deciding whether to open the map with `read_map` doesn't need them
/// yet.
fn headlines_body(map: &Map) -> String {
    let lines: Vec<String> = map.headlines().map(|node| format!("- {node}")).collect();
    format!(
        "Its {} nodes follow; read_map opens the rest, whole or around one node.\n{}",
        map.schema().headline_kinds.join(" and "),
        lines.join("\n")
    )
}

/// A `message.received` percept itself submitted, as `reflect` does.
fn is_percepts_prompt(event: &Event) -> bool {
    event.actor() == Actor::System && event.kind() == EventKind::MessageReceived
}

fn is_users_prompt(event: &Event) -> bool {
    event.actor() == Actor::User && event.kind() == EventKind::MessageReceived
}

/// Four characters a token: the one estimate the window, the index,
/// and `/context` all use.
fn estimate(chars: usize) -> usize {
    chars.div_ceil(4)
}

/// The message one event shows as in history: none for percept's own
/// prompt, a `reflect`, which replayed as system text would stand as
/// an instruction in every later turn; a tool result cut to its head;
/// otherwise what `message_of` gives.
fn history_message(event: &Event) -> Option<crate::harness::Message> {
    match event.payload() {
        _ if is_percepts_prompt(event) => None,
        crate::core::Payload::ToolResulted { content } => {
            Some(crate::harness::Message::ToolResult {
                content: cut(content, event.id()),
            })
        }
        _ => crate::harness::message_of(event),
    }
}

/// What `event` costs the model to read as history.
fn tokens(event: &Event) -> usize {
    history_message(event).map_or(0, |message| message_tokens(&message))
}

/// A message's text: its content, or a tool call's name and arguments.
fn text_of(message: &crate::harness::Message) -> String {
    match message {
        crate::harness::Message::Text { content, .. }
        | crate::harness::Message::ToolResult { content } => content.clone(),
        crate::harness::Message::ToolCall { tool, arguments } => format!("{tool} {arguments}"),
    }
}

/// The first `PREVIEW_CHARS` of `text`, on one line.
fn head(text: &str) -> String {
    text.chars()
        .take(PREVIEW_CHARS)
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect()
}

/// A tool result from an earlier turn, cut to its head with a handle
/// to the whole. A file read from a past turn is the costliest thing
/// in history and the least likely to be needed again as it was; the
/// id lets `read_event` open it when it is.
fn cut(content: &str, id: EventId) -> String {
    let len = content.chars().count();
    if len <= PREVIEW_CHARS {
        return content.to_string();
    }
    format!(
        "{}... [{len} chars; read_event {} opens the whole]",
        head(content),
        id.as_uuid()
    )
}

/// One line for an event past the window: who, the head of what, and
/// the id to open it. None for an event history would not show
/// either.
fn line(event: &Event) -> Option<String> {
    // A cited file is never a message, so it has no
    // `history_message` - it still names what was read, one line, same
    // as any other event past the window.
    if let crate::core::Payload::FileCited { path, lines, .. } = event.payload() {
        return Some(format!(
            "{} {}: read {}",
            event.id().as_uuid(),
            event.actor().name(),
            crate::core::cited_label(path, *lines)
        ));
    }
    let message = history_message(event)?;
    Some(format!(
        "{} {}: {}",
        event.id().as_uuid(),
        event.actor().name(),
        head(&text_of(&message))
    ))
}

/// How far back history goes in full: a share of the model's context
/// window. History fills to `share`, is cut back to `keep`, then holds
/// still until it fills again. A cut moves the request's prefix and
/// costs the provider's cache, so it lands rarely, and on a user
/// prompt, never mid-round. For a model that reports no window,
/// `floor` is the high mark and half of it the low.
#[derive(Clone, Copy)]
pub struct Window {
    pub share: f32,
    pub keep: f32,
    pub floor: u32,
}

impl Window {
    /// The high and low marks in tokens for a model with this window.
    fn marks(&self, context_window: Option<u32>) -> (usize, usize) {
        let mark = |share: f32, fallback: u32| {
            context_window.map_or(fallback as usize, |window| (window as f32 * share) as usize)
        };
        (
            mark(self.share, self.floor),
            mark(self.keep, self.floor / 2),
        )
    }

    /// Where history starts in `events`, the transcript before the
    /// turn in progress: the cuts replayed over all of it, so the
    /// start is a function of the log alone and two rounds agree on
    /// it without any state between them. The turn's own events are
    /// not counted: they are never history, and counting them would
    /// let a tool round move the start mid-turn.
    fn start(&self, events: &[Event], context_window: Option<u32>) -> usize {
        let (high, low) = self.marks(context_window);
        let mut start = 0;
        let mut held = 0;
        for (i, event) in events.iter().enumerate() {
            held += tokens(event);
            if held <= high {
                continue;
            }
            while start < i && held > low {
                held -= tokens(&events[start]);
                start += 1;
            }
        }
        // A cut lands on a user prompt: a window opening on a reply, a
        // call, or a result is a conversation the provider rejects or
        // the model misreads.
        while start < events.len() && !is_users_prompt(&events[start]) {
            start += 1;
        }
        start
    }
}

/// One part of the request `Context::build` assembles. Listed stable
/// first, the order `Harness::new` uses.
#[derive(Clone, Copy)]
pub enum Section {
    /// The harness's instructions as system text.
    Instructions,
    /// Every map, each as its schema's purpose line and this shape.
    Maps(MapShape),
    /// The transcript before the turn in progress, back as far as the
    /// window reaches, its tool results cut to a head. Before it, one
    /// line per event for up to `index` events further back, so the
    /// model can open with `read_event` what it can no longer see.
    History { window: Window, index: usize },
    /// The time, for `since` on the tools: the turn's prompt time
    /// while a turn streams, so every round of it sends the same
    /// text and the request only grows at its tail.
    Time,
    /// The turn in progress, whole. Never cut: a long tool loop must
    /// not evict the question it is answering.
    Turn,
}

impl fmt::Display for Section {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Section::Instructions => write!(f, "instructions"),
            Section::Maps(shape) => write!(
                f,
                "maps ({})",
                match shape {
                    MapShape::Prompt => "prompt",
                    MapShape::Headlines => "headlines",
                    MapShape::Tool => "tool",
                }
            ),
            Section::History { .. } => write!(f, "history"),
            Section::Time => write!(f, "time"),
            Section::Turn => write!(f, "turn"),
        }
    }
}

/// What `Context::build` needs from `App`'s state, borrowed for the
/// span of one build: `App` still decides whether tools go out and
/// whether the budget is spent; the context only renders.
pub struct View<'a> {
    pub instructions: Option<&'a str>,
    pub events: &'a [Event],
    pub schemas: &'a Schemas,
    pub scope: Scope,
    /// Where the turn now streaming began in `events`, if any.
    pub turn_start: Option<usize>,
    /// The model's context window in tokens, if it reports one.
    pub context_window: Option<u32>,
    /// The model's selected reasoning level for this session.
    pub reasoning_effort: Option<crate::harness::ReasoningEffort>,
    /// The tools to send with the request - already filtered by
    /// whether the model can use them and whether the turn's budget
    /// is spent.
    pub tools: Vec<crate::harness::ToolSpec>,
    /// Whether this turn has spent its tool budget: past it the
    /// request carries no tools, so the model must be told why.
    pub budget_spent: bool,
}

impl View<'_> {
    /// The transcript before the turn in progress, and the turn.
    fn split(&self) -> (&[Event], &[Event]) {
        self.events
            .split_at(self.turn_start.unwrap_or(self.events.len()))
    }
}

/// A list of sections and the render they produce together - see
/// `Section`.
pub struct Context {
    pub sections: Vec<Section>,
}

impl Context {
    /// The request for the next `reply`: each section's messages in
    /// list order, then the tools. Errs only when a map in the log does not
    /// fold, which is a corrupt log, not a bad turn.
    pub fn build(
        &self,
        view: View,
    ) -> Result<crate::harness::ModelRequest, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();
        for section in &self.sections {
            messages.extend(render(section, &view)?);
        }

        Ok(crate::harness::ModelRequest {
            messages,
            tools: view.tools,
            reasoning_effort: view.reasoning_effort,
        })
    }

    /// One line per section - its name and shape, how many messages it
    /// renders, and an estimate of their cost - for `/context` to show
    /// the model's actual input without sending it. Nothing here is
    /// committed to the log.
    pub fn describe(&self, view: &View) -> Result<String, Box<dyn std::error::Error>> {
        let mut lines = Vec::new();
        for section in &self.sections {
            let messages = render(section, view)?;
            let tokens: usize = messages.iter().map(message_tokens).sum();
            let word = if messages.len() == 1 {
                "message"
            } else {
                "messages"
            };
            let mut line = format!(
                "{:<16}{} {word}   {} tokens",
                section.to_string(),
                messages.len(),
                format_k(tokens)
            );
            if let Section::History { window, index } = section {
                let (high, low) = window.marks(view.context_window);
                line.push_str(&format!(
                    "  (fills to {}, cuts back to {}; index {index})",
                    format_k(high),
                    format_k(low)
                ));
            }
            lines.push(line);
        }
        Ok(lines.join("\n"))
    }
}

/// What `section` contributes to the request: `Context::build` is a
/// loop over this, and `describe` reads it to count and estimate one
/// section without sending anything.
fn render(
    section: &Section,
    view: &View,
) -> Result<Vec<crate::harness::Message>, Box<dyn std::error::Error>> {
    let mut messages = Vec::new();
    match section {
        // Before the maps: conventions frame how the maps are read,
        // and a system message the model sees first is the one it
        // weighs most. Every round, like the maps, so a long turn
        // never loses them to the window.
        Section::Instructions => {
            if let Some(instructions) = view.instructions {
                messages.push(crate::harness::Message::Text {
                    role: Actor::System,
                    content: format!(
                        "The project's instructions, which you follow when you read or \
                         change its files:\n\n{instructions}"
                    ),
                });
            }
        }
        Section::Maps(map_shape) => {
            // The maps sit outside the window on purpose: they are
            // what the model built so it need not hold the log, so
            // they are always in view. An empty map still goes in,
            // with its kinds: without them the model guesses at what
            // a node may be called and every `revise_map` call fails.
            // It says so in words that keep the log in play: the
            // model read a bare "(empty)" as "nothing was ever
            // decided" and stopped searching.
            for map in view.schemas.fold_all(&view.scope, view.events)? {
                let schema = map.schema();
                let body = if map.nodes().is_empty() {
                    "(empty: nothing has been recorded here yet. The log may still \
                     hold what it would.)"
                        .to_string()
                } else {
                    match map_shape {
                        MapShape::Prompt => map.to_string(),
                        MapShape::Headlines => headlines_body(&map),
                        MapShape::Tool => "read_map shows it.".to_string(),
                    }
                };
                messages.push(crate::harness::Message::Text {
                    role: Actor::System,
                    content: format!(
                        "The {} map: {}. {}. Node kinds: {}. Edge kinds: {}.\n{body}",
                        schema.name,
                        schema.purpose,
                        catalogue_line(&map),
                        schema.node_kinds_csv(),
                        schema.edge_kinds_csv()
                    ),
                });
            }
        }
        Section::History { window, index } => {
            let (before, _) = view.split();
            let start = window.start(before, view.context_window);
            // The index takes at most what history keeps after a cut,
            // newest first, so a page of long lines cannot outgrow
            // the history it introduces.
            let (_, low) = window.marks(view.context_window);
            let mut held = 0;
            let mut lines: Vec<String> = before[start.saturating_sub(*index)..start]
                .iter()
                .rev()
                .filter_map(line)
                .take_while(|line| {
                    held += estimate(line.chars().count());
                    held <= low
                })
                .collect();
            lines.reverse();
            if !lines.is_empty() {
                messages.push(crate::harness::Message::Text {
                    role: Actor::System,
                    content: format!(
                        "Before the messages below, oldest first, one line each; \
                         read_event opens any by its id:\n{}",
                        lines.join("\n")
                    ),
                });
            }
            messages.extend(before[start..].iter().filter_map(history_message));
        }
        Section::Time => {
            let (_, turn) = view.split();
            let at = turn.first().map_or_else(Timestamp::now, Event::created_at);
            messages.push(crate::harness::Message::Text {
                role: Actor::System,
                content: format!("The current time is {at}."),
            });
        }
        Section::Turn => {
            let (_, turn) = view.split();
            messages.extend(turn.iter().filter_map(crate::harness::message_of));
            // Dropping the tools is not enough on its own: a model
            // mid-turn reaches for one anyway, `begin_tool` stops the
            // turn on it, and the reply is empty. Say the budget is
            // spent so it answers.
            if view.budget_spent {
                messages.push(crate::harness::Message::Text {
                    role: Actor::System,
                    content: "This turn's tool budget is spent. You cannot call any more \
                              tools now. Answer with what you have."
                        .to_string(),
                });
            }
        }
    }
    Ok(messages)
}

/// What one message of the request costs to read.
fn message_tokens(message: &crate::harness::Message) -> usize {
    estimate(text_of(message).chars().count())
}

/// `n` as a plain number under a thousand, else in thousands or
/// millions to one decimal place - `1.2k`, `1.1M` - for a figure a
/// reader need not read exactly.
pub(crate) fn format_k(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests;
