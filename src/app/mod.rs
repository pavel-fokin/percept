use std::sync::Arc;

use crate::percept::{self, Actor, Event, EventId, Map, SCHEMAS};
use crate::shared::Timestamp;

/// Most tool calls one user turn may make. At the cap the next request
/// goes out with no tools, so the model has to answer with text.
const MAX_TOOL_CALLS: usize = 5;

/// How many of the most recent events the model reads as prompt text.
/// The log outgrows this; the transcript the TUI renders does not
/// shrink. A model that cannot hold the whole log has to search it,
/// which is what `search_events` is for.
const CONTEXT_EVENTS: usize = 20;

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
}

/// Orchestrates a chat: turns input into events, asks Model for a
/// reply, keeps the transcript. Every event goes through `log` before
/// it's added to `events`, so a failed write can never leave the
/// in-memory transcript ahead of what's durable.
pub struct App {
    events: Vec<Event>,
    /// The writer this app records as - stamped on every event it
    /// commits, so the log can tell its events from other writers'.
    source: String,
    chat: Arc<dyn percept::Model>,
    log: Arc<dyn percept::EventLog>,
    /// The tools the model may call, sent with each request when the
    /// model reports `tool_use`.
    tools: Vec<Arc<dyn percept::Tool>>,
    /// The turn now streaming, or None between turns.
    pending: Option<Turn>,
}

impl App {
    /// Opens on whatever `log` already holds, so the transcript
    /// survives a restart. A map that does not fold fails here, at
    /// open, the way a log line that does not decode does - not on the
    /// first turn.
    pub fn new(
        chat: Arc<dyn percept::Model>,
        log: Arc<dyn percept::EventLog>,
        tools: Vec<Arc<dyn percept::Tool>>,
        source: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let events = log.load()?;
        for schema in SCHEMAS {
            Map::fold(schema, &events)?;
        }

        Ok(Self {
            events,
            source,
            chat,
            log,
            tools,
            pending: None,
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
        for payload in output.commits {
            let event = Event::new(Actor::Model, self.source.clone(), Some(called_id), payload);
            self.commit(event)?;
        }
        let resulted = Event::tool_resulted(output.content, self.source.clone(), Some(called_id));
        let resulted_id = resulted.id();
        self.commit(resulted)?;
        self.with_pending(|turn| {
            turn.anchor = resulted_id;
            turn.tool_calls += 1;
        });
        Ok(())
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
    /// may be called and every `revise_map` call fails.
    fn build_request(&self) -> Result<percept::ModelRequest, Box<dyn std::error::Error>> {
        let mut messages = vec![percept::Message::Text {
            role: Actor::System,
            content: format!("The current time is {}.", Timestamp::now()),
        }];
        for schema in SCHEMAS {
            let map = Map::fold(schema, &self.events)?;
            let body = if map.nodes().is_empty() {
                "(empty)".to_string()
            } else {
                map.to_string()
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
        messages.extend(percept::to_messages(&self.events[self.window_start()..]));

        let tools = if self.chat.capabilities().tool_use && !self.tools_exhausted() {
            self.tools.iter().map(|tool| tool.spec()).collect()
        } else {
            Vec::new()
        };

        Ok(percept::ModelRequest { messages, tools })
    }

    /// Commits the thought then the reply buffered so far, both caused
    /// by the turn's `anchor`, and clears them. Leaves `pending` in
    /// place - the turn may not be over.
    fn flush_pending(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(turn) = self.pending.as_ref() else {
            return Ok(());
        };
        let cause = Some(turn.anchor);
        let thought = turn.thought.clone();
        let reply = turn.reply.clone();

        // A buffer is only cleared once its event is durable. Taking
        // the text first would leave a failed append with nothing to
        // retry from. The thought commits before the reply.
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
}

/// A buffer that has taken no chunks yet reads as nothing streaming,
/// so a caller never renders an empty turn.
fn text(buffer: Option<&String>) -> Option<&str> {
    buffer.map(String::as_str).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::percept::{Actor, Chunk, Payload};
    use crate::testing::{content, FakeLog, FakeTool, Scripted};

    const SOURCE: &str = "tui";

    fn thought(event: &Event) -> &str {
        match event.payload() {
            Payload::ThoughtRecorded { content } => content,
            _ => panic!("expected a thought.recorded event"),
        }
    }

    struct Silent;

    impl percept::Model for Silent {
        fn capabilities(&self) -> percept::ModelCapabilities {
            percept::ModelCapabilities {
                input: &[percept::Modality::Text],
                output: &[percept::Modality::Text],
                tool_use: false,
            }
        }

        fn reply(&self, _request: &percept::ModelRequest) -> percept::ReplyStream {
            Box::pin(tokio_stream::empty())
        }
    }

    #[test]
    fn streamed_reply_commits_one_event_caused_by_the_prompt() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        assert_eq!(app.events().len(), 1);
        assert!(app.pending_reply().is_none());

        app.append_chunk(Chunk::Reply("he".to_string()));
        app.append_chunk(Chunk::Reply("llo".to_string()));
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);

        app.end_stream().unwrap();
        assert!(app.pending_reply().is_none());

        let events = app.events();
        assert_eq!(events.len(), 2);
        assert!(events[0].actor() == Actor::User);
        assert!(events[1].actor() == Actor::Model);
        assert_eq!(content(&events[1]), "hello");
        assert!(events[1].causation_id() == Some(events[0].id()));
        assert_eq!(events[0].source(), SOURCE);
        assert_eq!(events[1].source(), SOURCE);
    }

    #[test]
    fn a_thought_and_a_reply_commit_as_two_model_events_thought_first() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hmm".to_string()));
        app.append_chunk(Chunk::Reply("hello".to_string()));
        assert_eq!(app.pending_thought(), Some("hmm"));
        assert_eq!(app.pending_reply(), Some("hello"));

        app.end_stream().unwrap();
        assert!(app.pending_thought().is_none());
        assert!(app.pending_reply().is_none());

        let events = app.events();
        assert_eq!(events.len(), 3);
        assert!(events[1].actor() == Actor::Model);
        assert_eq!(thought(&events[1]), "hmm");
        assert!(events[2].actor() == Actor::Model);
        assert_eq!(content(&events[2]), "hello");
        assert!(events[1].causation_id() == Some(events[0].id()));
        assert!(events[2].causation_id() == Some(events[0].id()));
    }

    #[test]
    fn a_submit_while_a_turn_streams_is_refused_and_records_nothing() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("first".to_string()).unwrap();
        assert!(app.is_replying());
        assert!(app.submit("second".to_string()).is_err());
        assert_eq!(app.events().len(), 1);

        app.append_chunk(Chunk::Reply("done".to_string()));
        app.end_stream().unwrap();
        assert!(!app.is_replying());
        assert!(app.submit("second".to_string()).is_ok());
    }

    #[test]
    fn a_turn_with_a_thought_and_no_reply_still_ends() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hm".to_string()));
        app.end_stream().unwrap();

        assert!(!app.is_replying());
        assert_eq!(app.events().len(), 2);
    }

    #[test]
    fn empty_reply_commits_nothing() {
        let mut app = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();
        let _ = app.submit("hi".to_string()).unwrap();
        app.end_stream().unwrap();
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn preseeded_log_becomes_the_opening_transcript() {
        let seeded = vec![
            Event::message_received(Actor::User, "hi".to_string(), SOURCE.to_string(), None),
            Event::message_received(Actor::Model, "hello".to_string(), SOURCE.to_string(), None),
        ];
        let log = Arc::new(FakeLog::seeded(seeded));
        let mut app = App::new(Arc::new(Silent), log, Vec::new(), SOURCE.to_string()).unwrap();
        assert_eq!(app.events().len(), 2);

        let _ = app.submit("next".to_string()).unwrap();
        assert_eq!(app.events().len(), 3);
    }

    #[test]
    fn append_failure_surfaces_as_err_and_leaves_transcript_unchanged() {
        let log = Arc::new(FakeLog::default());
        log.start_failing();
        let mut app = App::new(
            Arc::new(Silent),
            log.clone(),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        assert!(app.submit("hi".to_string()).is_err());
        assert!(app.events().is_empty());
    }

    #[test]
    fn a_failed_reply_append_leaves_the_reply_pending() {
        let log = Arc::new(FakeLog::default());
        let mut app = App::new(
            Arc::new(Silent),
            log.clone(),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Reply("hello".to_string()));
        log.start_failing();

        assert!(app.end_stream().is_err());
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);
    }

    #[test]
    fn a_failed_thought_append_leaves_the_reply_unattempted() {
        let log = Arc::new(FakeLog::default());
        let mut app = App::new(
            Arc::new(Silent),
            log.clone(),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();
        app.append_chunk(Chunk::Thought("hmm".to_string()));
        app.append_chunk(Chunk::Reply("hello".to_string()));
        log.start_failing();

        assert!(app.end_stream().is_err());
        assert_eq!(app.pending_thought(), Some("hmm"));
        assert_eq!(app.pending_reply(), Some("hello"));
        assert_eq!(app.events().len(), 1);
    }

    /// What tui::handle_stream does for one tool call: begin, then act
    /// on the `ToolStep` (running the tool inline instead of off-thread).
    fn run_one_tool(app: &mut App, name: &str, arguments: &str) {
        match app.begin_tool(name, arguments.to_string()).unwrap() {
            ToolStep::Run(tool, args) => {
                let output = run_tool(&*tool, &args);
                let _ = app.finish_tool(output).unwrap();
            }
            // `begin_tool` already committed the result (no such tool)
            // or ended the turn (cap spent).
            ToolStep::Continue(_) | ToolStep::Stop => {}
        }
    }

    #[test]
    fn a_tool_call_commits_called_then_resulted_then_the_reply() {
        let mut app = App::new(
            Arc::new(Scripted::new(vec![], true)),
            Arc::new(FakeLog::default()),
            vec![Arc::new(FakeTool)],
            SOURCE.to_string(),
        )
        .unwrap();

        // The sequence tui::handle_stream drives for one tool round.
        let _ = app.submit("what happened".to_string()).unwrap();
        run_one_tool(&mut app, "search_events", "{}");
        app.append_chunk(Chunk::Reply("found it".to_string()));
        app.end_stream().unwrap();

        let events = app.events();
        assert_eq!(events.len(), 4);
        assert!(events[1].actor() == Actor::Model);
        assert!(matches!(
            events[1].payload(),
            Payload::ToolCalled { tool, .. } if tool == "search_events"
        ));
        assert!(events[2].actor() == Actor::System);
        assert!(matches!(
            events[2].payload(),
            Payload::ToolResulted { content } if content == "ran"
        ));
        assert!(events[2].causation_id() == Some(events[1].id()));
        // The reply chains off the tool result, not the prompt.
        assert!(events[3].causation_id() == Some(events[2].id()));
        assert_eq!(content(&events[3]), "found it");
    }

    #[test]
    fn an_unknown_tool_name_becomes_the_result_content() {
        let mut app = App::new(
            Arc::new(Scripted::new(vec![], true)),
            Arc::new(FakeLog::default()),
            Vec::new(),
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("go".to_string()).unwrap();
        run_one_tool(&mut app, "nope", "{}");

        match app.events()[2].payload() {
            Payload::ToolResulted { content } => assert_eq!(content, "no such tool: nope"),
            _ => panic!("expected a tool.resulted event"),
        }
    }

    /// A tool whose output carries commits of its own, the way
    /// `revise_map` records what it judged from the log.
    struct RecordingTool;

    impl percept::Tool for RecordingTool {
        fn spec(&self) -> percept::ToolSpec {
            percept::ToolSpec {
                name: "search_events",
                description: "a fake that records two events",
                parameters: "{}",
            }
        }

        fn run(&self, _arguments: &str) -> Result<percept::ToolOutput, Box<dyn std::error::Error>> {
            Ok(percept::ToolOutput {
                content: "recorded two".to_string(),
                commits: vec![
                    Payload::MessageReceived {
                        content: "one".to_string(),
                    },
                    Payload::MessageReceived {
                        content: "two".to_string(),
                    },
                ],
            })
        }
    }

    #[test]
    fn a_tool_s_commits_land_between_the_call_and_the_result_caused_by_it() {
        let mut app = App::new(
            Arc::new(Scripted::new(vec![], true)),
            Arc::new(FakeLog::default()),
            vec![Arc::new(RecordingTool)],
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("go".to_string()).unwrap();
        run_one_tool(&mut app, "search_events", "{}");

        let events = app.events();
        assert_eq!(events.len(), 5);
        let called_id = events[1].id();
        assert!(matches!(events[1].payload(), Payload::ToolCalled { .. }));
        assert!(events[2].actor() == Actor::Model);
        assert_eq!(content(&events[2]), "one");
        assert!(events[2].causation_id() == Some(called_id));
        assert!(events[3].actor() == Actor::Model);
        assert_eq!(content(&events[3]), "two");
        assert!(events[3].causation_id() == Some(called_id));
        assert!(matches!(
            events[4].payload(),
            Payload::ToolResulted { content } if content == "recorded two"
        ));
        assert!(events[4].causation_id() == Some(called_id));
    }

    #[test]
    fn the_tool_call_limit_stops_tools_being_sent_and_then_exhausts() {
        let model = Arc::new(Scripted::new(vec![], true));
        let mut app = App::new(
            model.clone(),
            Arc::new(FakeLog::default()),
            vec![Arc::new(FakeTool)],
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("go".to_string()).unwrap();
        assert!(!app.tools_exhausted());
        for _ in 0..MAX_TOOL_CALLS {
            run_one_tool(&mut app, "search_events", "{}");
        }
        assert!(app.tools_exhausted());

        let counts = model.tool_counts();
        // submit, then one re-ask per finished tool call.
        assert_eq!(counts.len(), MAX_TOOL_CALLS + 1);
        assert!(counts[..MAX_TOOL_CALLS].iter().all(|&n| n == 1));
        // The request after the last allowed call carries no tools.
        assert_eq!(counts[MAX_TOOL_CALLS], 0);
    }

    #[test]
    fn a_model_that_cannot_use_tools_is_sent_none() {
        let model = Arc::new(Scripted::new(vec![vec![]], false));
        let mut app = App::new(
            model.clone(),
            Arc::new(FakeLog::default()),
            vec![Arc::new(FakeTool)],
            SOURCE.to_string(),
        )
        .unwrap();

        let _ = app.submit("hi".to_string()).unwrap();

        assert_eq!(model.tool_counts()[0], 0);
    }

    fn seeded_app(events: Vec<Event>, tools: Vec<Arc<dyn percept::Tool>>) -> (Arc<Scripted>, App) {
        let model = Arc::new(Scripted::new(vec![], true));
        let app = App::new(
            model.clone(),
            Arc::new(FakeLog::seeded(events)),
            tools,
            SOURCE.to_string(),
        )
        .unwrap();
        (model, app)
    }

    fn filler(n: usize) -> Vec<Event> {
        (0..n)
            .map(|i| Event::message_received(Actor::User, i.to_string(), SOURCE.to_string(), None))
            .collect()
    }

    #[test]
    fn a_log_longer_than_the_window_sends_only_its_newest_events() {
        let (model, mut app) = seeded_app(filler(25), Vec::new());

        let _ = app.submit("now".to_string()).unwrap();

        // The whole log stays in the transcript the TUI renders.
        assert_eq!(app.events().len(), 26);
        let sent = model.last_request();
        // The time, the decisions map, then the window.
        assert_eq!(sent.len(), CONTEXT_EVENTS + 2);
        assert!(!sent.contains(&"0".to_string()));
        assert!(sent.contains(&"24".to_string()));
        assert!(sent.contains(&"now".to_string()));
    }

    #[test]
    fn a_window_opening_on_a_tool_result_drops_it() {
        let mut events = vec![
            Event::tool_called(
                "search_events".to_string(),
                "{}".to_string(),
                SOURCE.to_string(),
                None,
            ),
            Event::tool_resulted("ran".to_string(), SOURCE.to_string(), None),
        ];
        events.extend(filler(CONTEXT_EVENTS - 2));

        // Submitting pushes the call out of the window, leaving its
        // result as the first event the model would otherwise see.
        let (model, mut app) = seeded_app(events, Vec::new());
        let _ = app.submit("now".to_string()).unwrap();

        let sent = model.last_request();
        assert!(!sent.contains(&"<result>".to_string()));
        assert_eq!(sent.len(), CONTEXT_EVENTS + 1);
    }

    #[test]
    fn a_long_tool_loop_never_evicts_the_prompt_it_is_answering() {
        let (model, mut app) = seeded_app(Vec::new(), vec![Arc::new(FakeTool)]);

        let _ = app.submit("the question".to_string()).unwrap();
        // Each round commits four events: thought, reply, call, result.
        for _ in 0..MAX_TOOL_CALLS {
            app.append_chunk(Chunk::Thought("hm".to_string()));
            app.append_chunk(Chunk::Reply("looking".to_string()));
            run_one_tool(&mut app, "search_events", "{}");
        }

        // The turn has outgrown the window on its own.
        assert!(app.events().len() > CONTEXT_EVENTS);
        assert!(model.last_request().contains(&"the question".to_string()));
    }

    #[test]
    fn a_map_is_sent_with_its_kinds_ahead_of_the_transcript_and_outside_the_window() {
        let mut events = vec![Event::new(
            Actor::User,
            SOURCE.to_string(),
            None,
            percept::Payload::NodeAdded {
                map: "decisions".to_string(),
                node: percept::NodeId::new(),
                kind: "decision".to_string(),
                name: "Rust over Go".to_string(),
                properties: Default::default(),
                sources: Vec::new(),
            },
        )];
        events.extend(filler(CONTEXT_EVENTS + 5));
        let (model, mut app) = seeded_app(events, Vec::new());

        let _ = app.submit("now".to_string()).unwrap();

        let sent = model.last_request();
        assert_eq!(sent.len(), CONTEXT_EVENTS + 2);
        assert!(sent[1].starts_with(
            "The decisions map, built from this log. Node kinds: question, option, \
             evidence, decision. Edge kinds: supports, contradicts, resolves.\n"
        ));
        assert!(sent[1].contains("- decision \"Rust over Go\""));
    }

    #[test]
    fn an_empty_map_is_still_sent_with_its_kinds() {
        let (model, mut app) = seeded_app(Vec::new(), Vec::new());

        let _ = app.submit("now".to_string()).unwrap();

        let sent = model.last_request();
        assert_eq!(sent.len(), 3);
        assert!(sent[1].contains("Node kinds: question, option, evidence, decision."));
        assert!(sent[1].ends_with("\n(empty)"));
    }

    #[test]
    fn a_map_that_does_not_fold_fails_at_open() {
        let events = vec![Event::new(
            Actor::User,
            SOURCE.to_string(),
            None,
            percept::Payload::NodeAdded {
                map: "decisions".to_string(),
                node: percept::NodeId::new(),
                kind: "goal".to_string(),
                name: "Ship".to_string(),
                properties: Default::default(),
                sources: Vec::new(),
            },
        )];
        let err = App::new(
            Arc::new(Silent),
            Arc::new(FakeLog::seeded(events)),
            Vec::new(),
            SOURCE.to_string(),
        )
        .err()
        .unwrap();

        assert!(err.to_string().contains("no node kind \"goal\""));
    }

    #[test]
    fn a_prompt_submitted_as_system_is_recorded_as_percepts_own() {
        let (_, mut app) = seeded_app(Vec::new(), Vec::new());

        let _ = app
            .submit_as(Actor::System, "revise the map".to_string())
            .unwrap();

        let prompt = app.events().last().unwrap();
        assert!(prompt.actor() == Actor::System);
        assert_eq!(content(prompt), "revise the map");
    }

    #[test]
    fn a_log_shorter_than_the_window_sends_all_of_it() {
        let (model, mut app) = seeded_app(filler(3), Vec::new());

        let _ = app.submit("now".to_string()).unwrap();

        // The time, the decisions map, three events, the prompt.
        assert_eq!(model.last_request().len(), 6);
    }
}
