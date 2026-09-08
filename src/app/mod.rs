use std::collections::HashSet;
use std::sync::Arc;

use context::{Context, Section, View, Window};

use crate::core::{Actor, Event, EventId, EventKind, Map, MapError, Source};

mod context;

/// Most tool calls one user turn may make, unless `Harness::tool_cap`
/// says otherwise. At the cap the next request goes out with no tools
/// and a note that the budget is spent, so the model answers with text
/// instead of reaching for a tool that is no longer there.
const MAX_TOOL_CALLS: usize = 5;

/// How much of the transcript the model reads as prompt text: an
/// eighth of its context window, cut back to a sixteenth. The log
/// outgrows this; the transcript the TUI renders does not shrink. A
/// model that cannot hold the whole log has to search it, which is
/// what `search_events` is for.
const WINDOW: Window = Window {
    share: 0.125,
    keep: 0.0625,
    floor: 8_000,
};

/// How many events before the window the model sees as one line each:
/// a page it can scan for what to open with `read_event`.
const INDEX_EVENTS: usize = 200;

/// How much of each cognitive map `Context::build` sends every turn.
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

/// What `App` is given besides the model, log, and renderer: the
/// tools it may call and how the turn around them is run. Built whole
/// in `main`, the only place concrete types are wired.
pub struct Harness {
    /// The tools the model may call, sent with each request when the
    /// model reports `tool_use`.
    pub tools: Vec<Arc<dyn crate::harness::Tool>>,
    /// Asked before any of `tools` runs. `AllowAll` unless `new` is
    /// overridden - the map tools have always run unasked.
    pub policy: Arc<dyn crate::harness::Policy>,
    /// Most tool calls one turn may make - see `MAX_TOOL_CALLS`.
    pub tool_cap: usize,
    /// Where the working tree is saved before each prompt, when the
    /// turn can change it. `None` for a chat over the log alone: a
    /// snapshot of a tree no tool touches would be noise.
    pub snapshot: Option<Arc<dyn crate::harness::Snapshot>>,
    /// The project's own instructions, sent as system text every
    /// round so the model works to the project's conventions. `None`
    /// for a chat over the log, which has no tree to follow them in.
    pub instructions: Option<String>,
    /// The shape of the request `App` sends every turn.
    pub context: Context,
}

impl Harness {
    /// `tools` and `map_shape` with today's defaults for the rest:
    /// `AllowAll`, `MAX_TOOL_CALLS`, no snapshot, no instructions, the
    /// standard context - instructions, maps, history back to `WINDOW`,
    /// the time, then the turn. Stable first: every provider reuses a
    /// request's prefix when it matches the last one, and a tool round
    /// only appends to the turn, so what changes goes last.
    pub fn new(tools: Vec<Arc<dyn crate::harness::Tool>>, map_shape: MapShape) -> Self {
        Self {
            tools,
            policy: Arc::new(crate::harness::AllowAll),
            tool_cap: MAX_TOOL_CALLS,
            snapshot: None,
            instructions: None,
            context: Context {
                sections: vec![
                    Section::Instructions,
                    Section::Maps(map_shape),
                    Section::History {
                        window: WINDOW,
                        index: INDEX_EVENTS,
                    },
                    Section::Time,
                    Section::Turn,
                ],
            },
        }
    }
}

/// What a presentation needs from the app layer - `tui` and `cli::ask`
/// both drive a turn through it. Lives here, not in either of them, so
/// implementing it doesn't pull a presentation into app's dependencies.
pub trait AppService {
    /// Records the user's message and returns a stream of the reply's
    /// chunks. Errs, without recording anything, if a turn is already
    /// streaming or if the event can't be appended to the log.
    fn submit(
        &mut self,
        text: String,
    ) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>> {
        self.submit_as(Actor::User, text)
    }

    /// `submit` with the prompt attributed to `actor`: `System` when
    /// percept itself asks, as `reflect` does, so the log never says
    /// the user asked something they did not.
    fn submit_as(
        &mut self,
        actor: Actor,
        text: String,
    ) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>>;

    /// Appends a chunk - thought or reply text - to the in-progress
    /// turn. Neither is an event yet - both are committed once by
    /// `end_stream`. Call only from the task that owns the App, never
    /// from inside the task draining the stream.
    fn append_chunk(&mut self, chunk: crate::harness::Chunk);

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
        output: crate::harness::ToolOutput,
    ) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>>;

    /// What the caller does when the user says no to a `ToolStep::Ask`:
    /// the refusal is committed as the call's result, in words the
    /// model can act on, and the model is asked again. The words are
    /// turn policy, so they live here and not in each presentation.
    fn decline_tool(&mut self) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>>;

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

    /// How far through its tool budget the streaming turn is: calls
    /// made so far, and the cap. `None` between turns.
    fn tool_progress(&self) -> Option<(usize, usize)>;

    /// Lets `tool` run unasked for the rest of the session - the user's
    /// standing answer once they have seen what it does. Session-only:
    /// nothing is committed to the log.
    fn allow_tool(&mut self, tool: &str);

    /// What the most recent round trip cost - set once the first
    /// `model.called` commits, and never before.
    fn last_usage(&self) -> Option<&crate::core::Usage>;

    fn context_window(&self) -> Option<u32>;

    /// The model's own name, available before any turn asks.
    fn model_name(&self) -> &str;

    /// The reasoning level in force for the next turn, when the active
    /// model has the control. `None` when it has none, or when it has
    /// one but no level is selected yet.
    fn reasoning_effort(&self) -> Option<crate::harness::ReasoningEffort>;

    /// The reasoning levels the active model allows, in order. Empty
    /// when it has no reasoning-effort control.
    fn supported_reasoning_efforts(&self) -> &'static [crate::harness::ReasoningEffort];

    /// Changes the reasoning effort for later turns. The caller gives
    /// a level from the active model's descriptor; unsupported models
    /// refuse it without changing the live model.
    fn set_reasoning_effort(
        &mut self,
        effort: crate::harness::ReasoningEffort,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Every model the catalog can reach, across providers.
    fn available_models(&self) -> crate::harness::ModelListing;

    /// Swaps the live model for the one `descriptor` names. Session-only:
    /// nothing is committed to the log. Refuses, leaving the current
    /// model in place, while a turn is streaming - a switch can never
    /// land mid-turn.
    fn set_model(
        &mut self,
        descriptor: &crate::harness::ModelDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// One line per section of the request the model would receive
    /// now, then a line with the last round's cost - see
    /// `Context::describe`. Nothing here is committed to the log.
    fn describe_context(&self) -> Result<String, Box<dyn std::error::Error>>;

    /// Puts the working tree back as it stood before the last finished
    /// turn, through the `Snapshot` taken at that turn's prompt. Errs
    /// while a turn streams, when no snapshot is kept, or when nothing
    /// is left to undo - a second `undo` in a row has no earlier
    /// snapshot to reach, since each turn's snapshot is used once.
    fn undo(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}

/// What the caller should do after `begin_tool`. The decision - run,
/// carry on, or stop - is `App`'s; the caller just spawns the work.
pub enum ToolStep {
    /// Run this tool with these arguments off the main loop, then pass
    /// its output to `finish_tool`.
    Run(Arc<dyn crate::harness::Tool>, String),
    /// The policy wants the user's say. Put the call to them; on yes,
    /// treat it as `Run`, on no, call `decline_tool`. `tool.called` is
    /// already committed either way: the log shows what the model
    /// asked for, and the result shows what the user let happen.
    Ask(Arc<dyn crate::harness::Tool>, String),
    /// Nothing to run (the name matched no tool); `App` already
    /// recorded the result. Drain this stream to continue the turn.
    Continue(crate::harness::ReplyStream),
    /// The per-turn tool cap is spent. `App` has already ended the
    /// turn: don't drain anything, and don't end it again.
    Stop,
}

/// Runs a tool, turning its failure into the text that stands as its
/// result. That substitution is turn policy - the string is committed
/// as `tool.resulted` content - so it lives here rather than in each
/// presentation that drives a turn. A failure commits nothing, the
/// same as `ToolOutput::text`'s empty `commits`.
pub fn run_tool(tool: &dyn crate::harness::Tool, arguments: &str) -> crate::harness::ToolOutput {
    tool.run(arguments)
        .unwrap_or_else(|err| crate::harness::ToolOutput::text(err.to_string()))
}

/// Whether `event` belongs in `App`'s own transcript cache: either it
/// is `source`'s own conversation - a message, a thought, a tool call -
/// or it changes a map, which stays the project's shared history no
/// matter who wrote it. A conversational event from another writer is
/// left out, so another client's dialogue never replays as this one's.
fn belongs_to_transcript(event: &Event, source: &Source) -> bool {
    event.source() == source || crate::core::map_of(event.payload()).is_some()
}

/// The index of the last `model.called` in `events`, so a reopened log
/// shows what its last round trip cost instead of reading as unasked.
fn last_model_called(events: &[Event]) -> Option<usize> {
    events
        .iter()
        .rposition(|event| event.kind() == EventKind::ModelCalled)
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
    /// The `tool.called` awaiting its result, with the tool's name, set
    /// by `begin_tool` and taken when the result commits.
    open_call: Option<(EventId, String)>,
    thought: String,
    reply: String,
    /// What the round trip just streamed cost, set by `append_chunk`
    /// and committed - and cleared - by `flush_pending`, after the
    /// thought and the reply it paid for.
    usage: Option<crate::core::Usage>,
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
    chat: Arc<dyn crate::harness::Model>,
    /// The reasoning level for the next turn: the model's configured
    /// default until `/effort` picks another, and reset to the new
    /// model's default on a switch that cannot keep the pick.
    reasoning_effort: Option<crate::harness::ReasoningEffort>,
    catalog: Arc<dyn crate::harness::ModelCatalog>,
    log: Arc<dyn crate::core::EventLog>,
    /// The tools, the policy and cap around them, the snapshot, the
    /// instructions, and the context - see `Harness`.
    harness: Harness,
    /// Tools the user has said always run, this session - checked
    /// before the harness's policy, which never learns.
    allowed: HashSet<String>,
    /// The prompt whose snapshot `undo` would restore: the last turn
    /// that took one, cleared once used.
    undo_point: Option<EventId>,
    /// Rerenders a map after a tool's commits change it - see
    /// `commit_tool_result`.
    renderer: Arc<dyn crate::core::MapRenderer>,
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
    /// transcript, though the search tools still reach them.
    /// `belongs_to_transcript` says what counts within this project. A
    /// map that does not fold fails here, at open, the way a log line
    /// that does not decode does - not on the first turn.
    pub fn new(
        chat: Arc<dyn crate::harness::Model>,
        catalog: Arc<dyn crate::harness::ModelCatalog>,
        log: Arc<dyn crate::core::EventLog>,
        harness: Harness,
        renderer: Arc<dyn crate::core::MapRenderer>,
        source: Source,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let scope = source.scope();
        let events: Vec<Event> = log
            .load()?
            .into_iter()
            .filter(|event| scope.admits(event) && belongs_to_transcript(event, &source))
            .collect();
        Map::fold_all(&scope, &events)?;
        let last_usage = last_model_called(&events);
        let reasoning_effort = chat.capabilities().default_reasoning_effort;

        Ok(Self {
            events,
            source,
            chat,
            reasoning_effort,
            catalog,
            log,
            harness,
            allowed: HashSet::new(),
            undo_point: None,
            renderer,
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

    /// The request state as `Context` sees it right now: what `ask`
    /// sends, and what `describe_context` reports on without sending
    /// anything.
    fn view(&self) -> View<'_> {
        let capabilities = self.chat.capabilities();
        let budget_spent = self.tools_exhausted();
        let tools = if capabilities.tool_use && !budget_spent {
            self.harness.tools.iter().map(|tool| tool.spec()).collect()
        } else {
            Vec::new()
        };
        View {
            instructions: self.harness.instructions.as_deref(),
            events: &self.events,
            scope: self.source.scope(),
            turn_start: self.pending.as_ref().map(|turn| turn.start),
            context_window: capabilities.context_window,
            reasoning_effort: self.reasoning_effort,
            tools,
            budget_spent,
        }
    }

    /// Starts the next reply stream for the current request state.
    /// Errs only when a map in the log does not fold - which is a
    /// corrupt log, not a bad turn.
    fn ask(&self) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>> {
        Ok(self.chat.reply(&self.harness.context.build(self.view())?))
    }

    /// Whether this turn has made `tool_cap` calls. Past it the request
    /// carries no tools and `begin_tool` ends the turn.
    fn tools_exhausted(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|turn| turn.tool_calls >= self.harness.tool_cap)
    }

    /// Commits each of `output.commits`, caused by the open call and
    /// attributed to the model - what the tool judged, before the
    /// result that reports it - then `tool.resulted` for the call
    /// itself, also caused by it, and advances the chain past it. No
    /// open call is a no-op.
    fn commit_tool_result(
        &mut self,
        output: crate::harness::ToolOutput,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let Some((called_id, _)) = self.pending.as_mut().and_then(|turn| turn.open_call.take())
        else {
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
                    .filter_map(|event| crate::core::map_of(event.payload()))
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
            let map = Map::fold(crate::core::Schema::find(name)?, &scope, &events)?;
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
    ) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>> {
        if self.pending.is_some() {
            return Err("a reply is already streaming".into());
        }
        let event = Event::message_received(actor, text, self.source.clone(), None);
        // The tree is saved before the prompt is on the record: a
        // snapshot that fails leaves the log without a prompt whose
        // changes could never be undone.
        if let Some(snapshot) = &self.harness.snapshot {
            snapshot.take(event.id())?;
            self.undo_point = Some(event.id());
        }
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

    fn append_chunk(&mut self, chunk: crate::harness::Chunk) {
        let Some(turn) = self.pending.as_mut() else {
            return;
        };
        match chunk {
            crate::harness::Chunk::Thought(text) => turn.thought.push_str(&text),
            crate::harness::Chunk::Reply(text) => turn.reply.push_str(&text),
            crate::harness::Chunk::Usage(usage) => turn.usage = Some(usage),
            // Every caller routes a tool call to `begin_tool`; it
            // never reaches here.
            crate::harness::Chunk::ToolCall { .. } => {}
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
        self.with_pending(|turn| turn.open_call = Some((called_id, tool.to_string())));

        let Some(run) = self
            .harness
            .tools
            .iter()
            .find(|t| t.spec().name == tool)
            .cloned()
        else {
            let output = crate::harness::ToolOutput::text(format!("no such tool: {tool}"));
            self.commit_tool_result(output)?;
            return Ok(ToolStep::Continue(self.ask()?));
        };
        if self.allowed.contains(tool) {
            return Ok(ToolStep::Run(run, arguments));
        }
        match self.harness.policy.check(tool, &arguments) {
            crate::harness::Verdict::Allow => Ok(ToolStep::Run(run, arguments)),
            crate::harness::Verdict::Ask => Ok(ToolStep::Ask(run, arguments)),
        }
    }

    fn finish_tool(
        &mut self,
        output: crate::harness::ToolOutput,
    ) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>> {
        self.commit_tool_result(output)?;
        self.ask()
    }

    fn decline_tool(&mut self) -> Result<crate::harness::ReplyStream, Box<dyn std::error::Error>> {
        let Some((_, tool)) = self
            .pending
            .as_ref()
            .and_then(|turn| turn.open_call.as_ref())
        else {
            return Err("no tool call is waiting".into());
        };
        let output = crate::harness::ToolOutput::text(format!(
            "The user declined to run {tool}. Do not retry it; ask them or do something else."
        ));
        self.finish_tool(output)
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

    fn tool_progress(&self) -> Option<(usize, usize)> {
        self.pending
            .as_ref()
            .map(|turn| (turn.tool_calls, self.harness.tool_cap))
    }

    fn allow_tool(&mut self, tool: &str) {
        self.allowed.insert(tool.to_string());
    }

    fn last_usage(&self) -> Option<&crate::core::Usage> {
        match self.events[self.last_usage?].payload() {
            crate::core::Payload::ModelCalled(usage) => Some(usage),
            _ => None,
        }
    }

    fn context_window(&self) -> Option<u32> {
        self.chat.capabilities().context_window
    }

    fn model_name(&self) -> &str {
        self.chat.name()
    }

    fn reasoning_effort(&self) -> Option<crate::harness::ReasoningEffort> {
        self.reasoning_effort
    }

    fn supported_reasoning_efforts(&self) -> &'static [crate::harness::ReasoningEffort] {
        self.chat.capabilities().reasoning_efforts
    }

    fn set_reasoning_effort(
        &mut self,
        effort: crate::harness::ReasoningEffort,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.chat.capabilities().reasoning_efforts.contains(&effort) {
            return Err(format!("{} does not support reasoning effort", self.model_name()).into());
        }
        self.reasoning_effort = Some(effort);
        Ok(())
    }

    fn available_models(&self) -> crate::harness::ModelListing {
        self.catalog.list()
    }

    fn set_model(
        &mut self,
        descriptor: &crate::harness::ModelDescriptor,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_replying() {
            return Err("a reply is already streaming".into());
        }
        let chat = self.catalog.build(descriptor)?;
        let capabilities = chat.capabilities();
        // Keep a pick the new model also allows; otherwise fall to its
        // own default.
        if !self
            .reasoning_effort
            .is_some_and(|effort| capabilities.reasoning_efforts.contains(&effort))
        {
            self.reasoning_effort = capabilities.default_reasoning_effort;
        }
        self.chat = chat;
        self.last_usage = None;
        Ok(())
    }

    fn describe_context(&self) -> Result<String, Box<dyn std::error::Error>> {
        let sections = self.harness.context.describe(&self.view())?;
        let last_round = match self.last_usage() {
            Some(usage) => format!(
                "last round: {} in, {} cached",
                context::format_k(usage.input_tokens as usize),
                context::format_k(usage.cached_tokens.unwrap_or(0) as usize)
            ),
            None => "last round: none yet".to_string(),
        };
        Ok(format!("{sections}\n{last_round}"))
    }

    fn undo(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_replying() {
            return Err("a reply is already streaming".into());
        }
        let Some(snapshot) = &self.harness.snapshot else {
            return Err("no snapshots are kept; run with PERCEPT_TOOLS=code".into());
        };
        let Some(prompt) = self.undo_point else {
            return Err("nothing to undo".into());
        };
        snapshot.restore(prompt)?;
        self.undo_point = None;
        // The restore put every rendered map back to before the turn,
        // while the log still holds what the turn added to them: the
        // log is the record, so the renders follow it, not the tree.
        let every_map = crate::core::schemas()
            .iter()
            .map(|schema| schema.name.clone())
            .collect();
        self.render_changed(&every_map)
    }
}

/// A buffer that has taken no chunks yet reads as nothing streaming,
/// so a caller never renders an empty turn.
fn text(buffer: Option<&String>) -> Option<&str> {
    buffer.map(String::as_str).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests;
