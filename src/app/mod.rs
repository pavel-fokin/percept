use std::collections::HashSet;
use std::sync::Arc;

use crate::percept::{self, Actor, Event, EventId, EventKind, Map, MapError, Source};
use crate::shared::Timestamp;

/// Most tool calls one user turn may make. At the cap the next request
/// goes out with no tools, so the model has to answer with text.
const MAX_TOOL_CALLS: usize = 5;

/// How many of the most recent events the model reads as prompt text.
/// The log outgrows this; the transcript the TUI renders does not
/// shrink. A model that cannot hold the whole log has to search it,
/// which is what `search_events` is for.
const CONTEXT_EVENTS: usize = 20;

/// How much of each cognitive map `build_request` sends every turn.
/// The map's kinds go in regardless of shape - `revise_map` needs them
/// to check a change before it commits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapShape {
    /// The whole map, as today.
    Prompt,
    /// Only its headline nodes; `read_map` opens the rest.
    Headlines,
    /// Only its size; `read_map` opens it.
    Tool,
}

impl MapShape {
    /// Whether the model needs `read_map` to see a whole map.
    pub fn opens_by_tool(self) -> bool {
        matches!(self, Self::Headlines | Self::Tool)
    }
}

/// What a presentation needs from the app layer - `tui` and `cli::ask`
/// both drive a turn through it. Lives here, not in either of them, so
/// implementing it doesn't pull a presentation into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a stream of the reply's
    /// chunks. Errs, without recording anything, if a turn is already
    /// streaming or if the event can't be appended to the log.
    fn submit(&mut self, text: String) -> Result<percept::ReplyStream, Box<dyn std::error::Error>> {
        self.submit_as(Actor::User, text)
    }

    /// `submit` with the prompt attributed to `actor`: `System` when
    /// percept itself asks, as `reflect` does, so the log never says
    /// the user asked something they did not.
    fn submit_as(
        &mut self,
        actor: Actor,
        text: String,
    ) -> Result<percept::ReplyStream, Box<dyn std::error::Error>>;

    /// Appends a chunk - thought or reply text - to the in-progress
    /// turn. Neither is an event yet - both are committed once by
    /// `end_stream`. Call only from the task that owns the App, never
    /// from inside the task draining the stream.
    fn append_chunk(&mut self, chunk: percept::Chunk);

    /// Records the model's `tool.called` (after whatever it said first)
    /// and decides what happens next - see `ToolStep`. The turn's
    /// policy lives here, not in the caller: the caller only carries
    /// out the step.
    fn begin_tool(
        &mut self,
        tool: &str,
        arguments: String,
    ) -> Result<ToolStep, Box<dyn std::error::Error>>;

    /// Commits whatever a tool call produced - the payloads it asked to
    /// record, then `tool.resulted` with its text (or the error it
    /// failed with) - then asks the model again with it in the
    /// history. Returns the next reply stream - still the same user
    /// turn.
    fn finish_tool(
        &mut self,
        output: percept::ToolOutput,
    ) -> Result<percept::ReplyStream, Box<dyn std::error::Error>>;

    /// Commits the streamed thought, if any, then the streamed reply, if
    /// any, as separate model events. Either with no chunks commits
    /// nothing. Errs if an event can't be appended to the log; a failed
    /// thought append leaves the reply uncommitted too.
    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>>;

    fn events(&self) -> &[Event];

    /// The reply now streaming, if any - not yet in `events`.
    fn pending_reply(&self) -> Option<&str>;

    /// The thought now streaming, if any - not yet in `events`.
    fn pending_thought(&self) -> Option<&str>;

    /// Whether a turn is still streaming. A second `submit` before it
    /// ends would overwrite the first turn's cause and fuse both
    /// replies into one event, and an append-only log keeps the damage.
    fn is_replying(&self) -> bool;

    /// What the most recent round trip cost - set once the first
    /// `model.called` commits, and never before.
    fn last_usage(&self) -> Option<&percept::Usage>;

    fn context_window(&self) -> Option<u32>;

    /// The model's own name, available before any turn asks.
    fn model_name(&self) -> &str;

    /// Every model the catalog can reach, across providers.
    fn available_models(&self) -> percept::ModelListing;

    /// Swaps the live model for the one `descriptor` names. Session-only:
    /// nothing is committed to the log. Refuses, leaving the current
    /// model in place, while a turn is streaming - a switch can never
    /// land mid-turn.
    fn set_model(
        &mut self,
        descriptor: &percept::ModelDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

/// What the caller should do after `begin_tool`. The decision - run,
/// carry on, or stop - is `App`'s; the caller just spawns the work.
pub enum ToolStep {
    /// Run this tool with these arguments off the main loop, then pass
    /// its output to `finish_tool`.
    Run(Arc<dyn percept::Tool>, String),
    /// Nothing to run (the name matched no tool); `App` already
    /// recorded the result. Drain this stream to continue the turn.
    Continue(percept::ReplyStream),
    /// The per-turn tool cap is spent. `App` has already ended the
    /// turn: don't drain anything, and don't end it again.
    Stop,
}

/// Runs a tool, turning its failure into the text that stands as its
/// result. That substitution is turn policy - the string is committed
/// as `tool.resulted` content - so it lives here rather than in each
/// presentation that drives a turn. A failure commits nothing, the
/// same as `ToolOutput::text`'s empty `commits`.
pub fn run_tool(tool: &dyn percept::Tool, arguments: &str) -> percept::ToolOutput {
    tool.run(arguments)
        .unwrap_or_else(|err| percept::ToolOutput::text(err.to_string()))
}

/// A `message.received` percept itself submitted, as `reflect` does.
fn is_percepts_prompt(event: &Event) -> bool {
    event.actor() == Actor::System && event.kind() == EventKind::MessageReceived
}

/// The index of the last `model.called` in `events`, so a reopened log
/// shows what its last round trip cost instead of reading as unasked.
fn last_model_called(events: &[Event]) -> Option<usize> {
    events
        .iter()
        .rposition(|event| event.kind() == EventKind::ModelCalled)
}

/// `MapShape::Headlines`'s body: the headline nodes as `Map`'s
/// `Display` formats a node line, without properties - a reader
/// deciding whether to open the map with `read_map` doesn't need them
/// yet.
fn headlines_body(map: &Map) -> String {
    let lines: Vec<String> = map.headlines().map(|node| format!("- {node}")).collect();
    format!(
        "Its {} nodes follow; read_map shows the whole map.\n{}",
        map.schema().headline_kinds.join(" and "),
        lines.join("\n")
    )
}

/// The turn now streaming. `anchor` is what the next model events are
/// caused by: the prompt at first, then each `tool.resulted` as the
/// loop advances. A thought and a reply share it; a tool call moves it
/// on. One value, so the chain can't outlive the buffers it belongs to.
struct Turn {
    anchor: EventId,
    /// Where this turn's events begin in `App::events`. `anchor` moves
    /// as the tool loop advances; this does not, so the window can
    /// always reach back to the question being answered. An index is
    /// exact because the transcript is only ever appended to.
    start: usize,
    tool_calls: usize,
    /// The `tool.called` awaiting its result, set by `begin_tool` and
    /// taken when the result commits.
    open_call: Option<EventId>,
    thought: String,
    reply: String,
    /// What the round trip just streamed cost, set by `append_chunk`
    /// and committed - and cleared - by `flush_pending`, after the
    /// thought and the reply it paid for.
    usage: Option<percept::Usage>,
}

/// Orchestrates a chat: turns input into events, asks Model for a
/// reply, keeps the transcript. Every event goes through `log` before
/// it's added to `events`, so a failed write can never leave the
/// in-memory transcript ahead of what's durable.
pub struct App {
    events: Vec<Event>,
    /// The writer this app records as - stamped on every event it
    /// commits, so the log can tell its events from other writers'.
    source: Source,
    chat: Arc<dyn percept::Model>,
    catalog: Arc<dyn percept::ModelCatalog>,
    log: Arc<dyn percept::EventLog>,
    /// The tools the model may call, sent with each request when the
    /// model reports `tool_use`.
    tools: Vec<Arc<dyn percept::Tool>>,
    /// Rerenders a map after a tool's commits change it - see
    /// `commit_tool_result`.
    renderer: Arc<dyn percept::MapRenderer>,
    /// How much of each map `build_request` sends every turn.
    map_shape: MapShape,
    /// The turn now streaming, or None between turns.
    pending: Option<Turn>,
    /// Where the most recent `model.called` landed in `events` - the
    /// last round trip's cost, not a running total. Set from the loaded
    /// log at `new`, same as a turn committed this session would set
    /// it; `None` only when the log holds no `model.called` at all.
    last_usage: Option<usize>,
}

impl App {
    /// Opens on what `log` already holds for this project, so the
    /// transcript survives a restart. The log is shared by every
    /// project; another project's events stay in it and out of this
    /// transcript, though the search tools still reach them. A map
    /// that does not fold fails here, at open, the way a log line that
    /// does not decode does - not on the first turn.
    pub fn new(
        chat: Arc<dyn percept::Model>,
        catalog: Arc<dyn percept::ModelCatalog>,
        log: Arc<dyn percept::EventLog>,
        tools: Vec<Arc<dyn percept::Tool>>,
        renderer: Arc<dyn percept::MapRenderer>,
        map_shape: MapShape,
        source: Source,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let scope = source.scope();
        let events: Vec<Event> = log
            .load()?
            .into_iter()
            .filter(|event| scope.admits(event))
            .collect();
        Map::fold_all(&scope, &events)?;
        let last_usage = last_model_called(&events);

        Ok(Self {
            events,
            source,
            chat,
            catalog,
            log,
            tools,
            renderer,
            map_shape,
            pending: None,
            last_usage,
        })
    }

    /// Appends an event, then adds it to the transcript - never the
    /// other way round, so a failed write can't leave the transcript
    /// ahead of what's durable.
    fn commit(&mut self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        self.log.append(&event)?;
        self.events.push(event);
        Ok(())
    }

    /// Runs `f` on the streaming turn, or nothing if none is. Lets a
    /// caller touch `Turn` right after `commit` without re-nesting the
    /// borrow each time.
    fn with_pending(&mut self, f: impl FnOnce(&mut Turn)) {
        if let Some(turn) = self.pending.as_mut() {
            f(turn);
        }
    }

    /// Starts the next reply stream for the current request state.
    /// Errs only when a map in the log does not fold - which is a
    /// corrupt log, not a bad turn.
    fn ask(&self) -> Result<percept::ReplyStream, Box<dyn std::error::Error>> {
        Ok(self.chat.reply(&self.build_request()?))
    }

    /// Whether this turn has made `MAX_TOOL_CALLS`. Past it the request
    /// carries no tools and `begin_tool` ends the turn.
    fn tools_exhausted(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|turn| turn.tool_calls >= MAX_TOOL_CALLS)
    }

    /// Commits each of `output.commits`, caused by the open call and
    /// attributed to the model - what the tool judged, before the
    /// result that reports it - then `tool.resulted` for the call
    /// itself, also caused by it, and advances the chain past it. No
    /// open call is a no-op.
    fn commit_tool_result(
        &mut self,
        output: percept::ToolOutput,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some(called_id) = self.pending.as_mut().and_then(|turn| turn.open_call.take()) else {
            return Ok(());
        };
        let commits: Vec<Event> = output
            .commits
            .into_iter()
            .map(|payload| Event::new(Actor::Model, self.source.clone(), Some(called_id), payload))
            .collect();
        // A tool checked its commits against the log file, and this
        // transcript can be behind it - another writer since startup.
        // Checked again here, against what the next request will fold,
        // so a mismatch reaches the model as the call's result instead
        // of ending the run at the next `build_request`.
        let (content, changed) = match self.fits_maps(&commits) {
            Ok(()) => {
                let changed: HashSet<String> = commits
                    .iter()
                    .filter_map(|event| percept::map_of(event.payload()))
                    .map(str::to_string)
                    .collect();
                for event in commits {
                    self.commit(event)?;
                }
                (output.content, changed)
            }
            Err(err) => (err.to_string(), HashSet::new()),
        };
        let resulted = Event::tool_resulted(content, self.source.clone(), Some(called_id));
        let resulted_id = resulted.id();
        self.commit(resulted)?;
        self.with_pending(|turn| {
            turn.anchor = resulted_id;
            turn.tool_calls += 1;
        });
        self.render_changed(&changed)
    }

    /// Rerenders every map named in `changed`. Runs once `tool.resulted`
    /// is committed, so a render that fails leaves the log whole: the
    /// map is in the log, and only its view is stale. Folds from the
    /// log file, not this transcript, because another writer may have
    /// appended since startup and the render must not lose what it
    /// wrote. A tool round that touched no map folds and renders
    /// nothing.
    fn render_changed(&self, changed: &HashSet<String>) -> Result<(), Box<dyn std::error::Error>> {
        if changed.is_empty() {
            return Ok(());
        }
        let events = self.log.load()?;
        let scope = self.source.scope();
        for name in changed {
            let map = Map::fold(percept::Schema::find(name)?, &scope, &events)?;
            self.renderer.render(&map)?;
        }
        Ok(())
    }

    /// Whether every map still folds once `new` follows the transcript.
    fn fits_maps(&self, new: &[Event]) -> Result<(), MapError> {
        if new.is_empty() {
            return Ok(());
        }
        Map::fold_all(&self.source.scope(), self.events.iter().chain(new)).map(drop)
    }

    /// Where the model's view starts: `CONTEXT_EVENTS` back from the
    /// end, or the start of the turn in progress if that is older. A
    /// tool round commits up to four events, so a loop that runs to
    /// `MAX_TOOL_CALLS` would otherwise evict the question it is
    /// answering. The turn in progress is never history.
    fn window_start(&self) -> usize {
        let tail = self.events.len().saturating_sub(CONTEXT_EVENTS);
        match self.pending.as_ref() {
            Some(turn) => tail.min(turn.start),
            None => tail,
        }
    }

    /// The request for the next `reply`: the current time, then each
    /// map with the kinds its schema allows, then the windowed
    /// transcript, then the tools - dropped once a turn hits
    /// `MAX_TOOL_CALLS` or the model can't use them, so the model is
    /// forced to a text answer. The maps sit outside the window on
    /// purpose: they are what the model built so it need not hold the
    /// log, so they are always in view. An empty map still goes in,
    /// with its kinds: without them the model guesses at what a node
    /// may be called and every `revise_map` call fails. It says so in
    /// words that keep the log in play: the model read a bare
    /// "(empty)" as "nothing was ever decided" and stopped searching.
    fn build_request(&self) -> Result<percept::ModelRequest, Box<dyn std::error::Error>> {
        let mut messages = vec![percept::Message::Text {
            role: Actor::System,
            content: format!("The current time is {}.", Timestamp::now()),
        }];
        for map in Map::fold_all(&self.source.scope(), &self.events)? {
            let schema = map.schema();
            let body = if map.nodes().is_empty() {
                "(empty: nothing has been recorded here yet. The log may still hold what it would.)"
                    .to_string()
            } else {
                match self.map_shape {
                    MapShape::Prompt => map.to_string(),
                    MapShape::Headlines => headlines_body(&map),
                    MapShape::Tool => format!(
                        "It holds {} nodes and {} edges. read_map shows it.",
                        map.nodes().len(),
                        map.edges().len()
                    ),
                }
            };
            messages.push(percept::Message::Text {
                role: Actor::System,
                content: format!(
                    "The {} map, built from this log. Node kinds: {}. Edge kinds: {}.\n{body}",
                    schema.name,
                    schema.node_kinds.join(", "),
                    schema.edge_kinds.join(", ")
                ),
            });
        }
        // Percept's own prompts - a `reflect` - are history the model
        // need not obey. Replayed as system text they would stand as an
        // instruction in every later turn, so before this turn they are
        // dropped; the turn's own prompt stays.
        let turn_start = self
            .pending
            .as_ref()
            .map_or(self.events.len(), |turn| turn.start);
        let history = self.events[self.window_start()..turn_start]
            .iter()
            .filter(|event| !is_percepts_prompt(event));
        messages.extend(percept::to_messages(
            history.chain(&self.events[turn_start..]),
        ));

        let tools = if self.chat.capabilities().tool_use && !self.tools_exhausted() {
            self.tools.iter().map(|tool| tool.spec()).collect()
        } else {
            Vec::new()
        };

        Ok(percept::ModelRequest { messages, tools })
    }

    /// Commits the thought then the reply buffered so far, then the
    /// round trip's `model.called`, all caused by the turn's `anchor`,
    /// and clears them. Leaves `pending` in place - the turn may not be
    /// over.
    fn flush_pending(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(turn) = self.pending.as_ref() else {
            return Ok(());
        };
        let cause = Some(turn.anchor);
        let thought = turn.thought.clone();
        let reply = turn.reply.clone();
        let usage = turn.usage.clone();

        // A buffer is only cleared once its event is durable. Taking
        // the text first would leave a failed append with nothing to
        // retry from. The thought commits before the reply, before the
        // usage that paid for both.
        if !thought.is_empty() {
            let event = Event::thought_recorded(Actor::Model, thought, self.source.clone(), cause);
            self.commit(event)?;
            self.with_pending(|turn| turn.thought.clear());
        }
        if !reply.is_empty() {
            let event = Event::message_received(Actor::Model, reply, self.source.clone(), cause);
            self.commit(event)?;
            self.with_pending(|turn| turn.reply.clear());
        }
        if let Some(usage) = usage {
            let event = Event::model_called(usage, self.source.clone(), cause);
            self.commit(event)?;
            self.with_pending(|turn| turn.usage = None);
            self.last_usage = Some(self.events.len() - 1);
        }
        Ok(())
    }
}

impl AppService for App {
    fn submit_as(
        &mut self,
        actor: Actor,
        text: String,
    ) -> Result<percept::ReplyStream, Box<dyn std::error::Error>> {
        if self.pending.is_some() {
            return Err("a reply is already streaming".into());
        }
        let event = Event::message_received(actor, text, self.source.clone(), None);
        self.log.append(&event)?;
        let anchor = event.id();
        let start = self.events.len();
        self.events.push(event);
        self.pending = Some(Turn {
            anchor,
            start,
            tool_calls: 0,
            open_call: None,
            thought: String::new(),
            reply: String::new(),
            usage: None,
        });

        self.ask()
    }

    fn append_chunk(&mut self, chunk: percept::Chunk) {
        let Some(turn) = self.pending.as_mut() else {
            return;
        };
        match chunk {
            percept::Chunk::Thought(text) => turn.thought.push_str(&text),
            percept::Chunk::Reply(text) => turn.reply.push_str(&text),
            percept::Chunk::Usage(usage) => turn.usage = Some(usage),
            // Every caller routes a tool call to `begin_tool`; it
            // never reaches here.
            percept::Chunk::ToolCall { .. } => {}
        }
    }

    fn begin_tool(
        &mut self,
        tool: &str,
        arguments: String,
    ) -> Result<ToolStep, Box<dyn std::error::Error>> {
        // A model that keeps calling past the cap would loop forever;
        // end the turn instead.
        if self.tools_exhausted() {
            self.end_stream()?;
            return Ok(ToolStep::Stop);
        }

        // Whatever the model said before the call is real and commits
        // first, so the call's cause is the text that led to it.
        self.flush_pending()?;

        let cause = self.pending.as_ref().map(|turn| turn.anchor);
        let called = Event::tool_called(
            tool.to_string(),
            arguments.clone(),
            self.source.clone(),
            cause,
        );
        let called_id = called.id();
        self.commit(called)?;
        self.with_pending(|turn| turn.open_call = Some(called_id));

        match self.tools.iter().find(|t| t.spec().name == tool).cloned() {
            Some(run) => Ok(ToolStep::Run(run, arguments)),
            None => {
                let output = percept::ToolOutput::text(format!("no such tool: {tool}"));
                self.commit_tool_result(output)?;
                Ok(ToolStep::Continue(self.ask()?))
            }
        }
    }

    fn finish_tool(
        &mut self,
        output: percept::ToolOutput,
    ) -> Result<percept::ReplyStream, Box<dyn std::error::Error>> {
        self.commit_tool_result(output)?;
        self.ask()
    }

    fn end_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.flush_pending()?;
        self.pending = None;
        Ok(())
    }

    fn events(&self) -> &[Event] {
        &self.events
    }

    fn pending_reply(&self) -> Option<&str> {
        text(self.pending.as_ref().map(|turn| &turn.reply))
    }

    fn pending_thought(&self) -> Option<&str> {
        text(self.pending.as_ref().map(|turn| &turn.thought))
    }

    fn is_replying(&self) -> bool {
        self.pending.is_some()
    }

    fn last_usage(&self) -> Option<&percept::Usage> {
        match self.events[self.last_usage?].payload() {
            percept::Payload::ModelCalled(usage) => Some(usage),
            _ => None,
        }
    }

    fn context_window(&self) -> Option<u32> {
        self.chat.capabilities().context_window
    }

    fn model_name(&self) -> &str {
        self.chat.name()
    }

    fn available_models(&self) -> percept::ModelListing {
        self.catalog.list()
    }

    fn set_model(
        &mut self,
        descriptor: &percept::ModelDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_replying() {
            return Err("a reply is already streaming".into());
        }
        self.chat = self.catalog.build(descriptor)?;
        self.last_usage = None;
        Ok(())
    }
}

/// A buffer that has taken no chunks yet reads as nothing streaming,
/// so a caller never renders an empty turn.
fn text(buffer: Option<&String>) -> Option<&str> {
    buffer.map(String::as_str).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests;
