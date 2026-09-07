use crate::app::MapShape;
use crate::percept::{self, Actor, Event, EventKind, Map, Scope};
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

/// What `event` costs the model to read, at four characters a token.
/// Zero for an event `to_messages` drops, so the window is measured in
/// what the model sees.
fn tokens(event: &Event) -> usize {
    let chars = match event.payload() {
        percept::Payload::MessageReceived { content }
        | percept::Payload::ToolResulted { content } => content.chars().count(),
        percept::Payload::ToolCalled { tool, arguments } => {
            tool.chars().count() + arguments.chars().count()
        }
        _ => 0,
    };
    chars.div_ceil(4)
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
    /// Where history starts: the cuts replayed over the whole
    /// transcript, so the start is a function of the log alone and two
    /// rounds agree on it without any state between them.
    fn start(&self, events: &[Event], context_window: Option<u32>) -> usize {
        let mark = |share: f32, fallback: u32| {
            context_window.map_or(fallback, |window| (window as f32 * share) as u32) as usize
        };
        let (high, low) = (
            mark(self.share, self.floor),
            mark(self.keep, self.floor / 2),
        );
        let mut start = 0;
        let mut held = 0;
        for (i, event) in events.iter().enumerate() {
            held += tokens(event);
            if held <= high {
                continue;
            }
            while start < i && (held > low || !is_users_prompt(&events[start])) {
                held -= tokens(&events[start]);
                start += 1;
            }
        }
        start
    }
}

/// One part of the request `Context::build` assembles. Order and
/// membership are data, so a new purpose is a new list, not a new
/// function.
pub enum Section {
    /// The current time, for `since` on the tools. It changes every
    /// round, so it goes after everything a provider could cache.
    Time,
    /// The harness's instructions as system text.
    Instructions,
    /// Every map, each as its schema's purpose line and this shape.
    Maps(MapShape),
    /// The transcript in full, back as far as the window reaches, plus
    /// the whole turn in progress.
    History(Window),
}

/// What `Context::build` needs from `App`'s state, borrowed for the
/// span of one build: `App` still decides whether tools go out and
/// whether the budget is spent; the context only renders.
pub struct View<'a> {
    pub instructions: Option<&'a str>,
    pub events: &'a [Event],
    pub scope: Scope,
    /// Where the turn now streaming began in `events`, if any.
    pub turn_start: Option<usize>,
    /// The model's context window in tokens, if it reports one.
    pub context_window: Option<u32>,
    /// The tools to send with the request - already filtered by
    /// whether the model can use them and whether the turn's budget
    /// is spent.
    pub tools: Vec<percept::ToolSpec>,
    /// Whether this turn has spent its tool budget: past it the
    /// request carries no tools, so the model must be told why.
    pub budget_spent: bool,
}

/// A list of sections and the render they produce together - see
/// `Section`.
pub struct Context {
    pub sections: Vec<Section>,
}

impl Context {
    /// The request for the next `reply`: each section's messages in
    /// list order, then the tools, then the budget-spent note if the
    /// turn is at its cap. Errs only when a map in the log does not
    /// fold, which is a corrupt log, not a bad turn.
    pub fn build(&self, view: View) -> Result<percept::ModelRequest, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();
        for section in &self.sections {
            match section {
                Section::Time => messages.push(percept::Message::Text {
                    role: Actor::System,
                    content: format!("The current time is {}.", Timestamp::now()),
                }),
                // Before the maps: conventions frame how the maps are
                // read, and a system message the model sees first is
                // the one it weighs most. Every round, like the maps,
                // so a long turn never loses them to the window.
                Section::Instructions => {
                    if let Some(instructions) = view.instructions {
                        messages.push(percept::Message::Text {
                            role: Actor::System,
                            content: format!(
                                "The project's instructions, which you follow when you read or \
                                 change its files:\n\n{instructions}"
                            ),
                        });
                    }
                }
                Section::Maps(map_shape) => {
                    // The maps sit outside the window on purpose: they
                    // are what the model built so it need not hold the
                    // log, so they are always in view. An empty map
                    // still goes in, with its kinds: without them the
                    // model guesses at what a node may be called and
                    // every `revise_map` call fails. It says so in
                    // words that keep the log in play: the model read
                    // a bare "(empty)" as "nothing was ever decided"
                    // and stopped searching.
                    for map in Map::fold_all(&view.scope, view.events)? {
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
                        messages.push(percept::Message::Text {
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
                Section::History(window) => {
                    // The turn in progress is never history: a long
                    // tool loop must not evict the question it is
                    // answering.
                    let turn_start = view.turn_start.unwrap_or(view.events.len());
                    let window_start = window
                        .start(view.events, view.context_window)
                        .min(turn_start);
                    // Percept's own prompts - a `reflect` - are
                    // history the model need not obey. Replayed as
                    // system text they would stand as an instruction
                    // in every later turn, so before this turn they
                    // are dropped; the turn's own prompt stays.
                    let history = view.events[window_start..turn_start]
                        .iter()
                        .filter(|event| !is_percepts_prompt(event));
                    messages.extend(percept::to_messages(
                        history.chain(&view.events[turn_start..]),
                    ));
                }
            }
        }

        // Dropping the tools is not enough on its own: a model
        // mid-turn reaches for one anyway, `begin_tool` stops the
        // turn on it, and the reply is empty. Say the budget is spent
        // so it answers.
        if view.budget_spent {
            messages.push(percept::Message::Text {
                role: Actor::System,
                content: "This turn's tool budget is spent. You cannot call any more \
                          tools now. Answer with what you have."
                    .to_string(),
            });
        }

        Ok(percept::ModelRequest {
            messages,
            tools: view.tools,
        })
    }
}

#[cfg(test)]
mod tests;
