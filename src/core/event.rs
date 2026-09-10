use std::collections::BTreeMap;
use std::path::PathBuf;

use super::NodeId;
use crate::shared::{Id, Timestamp};

/// Identifies an Event.
pub type EventId = Id<Event>;

/// Marker for `HumanId` - no `Human` entity is stored in the domain;
/// this only pins the id's phantom type, the way `Event` and `Node` do
/// for their own ids.
pub enum Human {}

/// Identifies the person at the keyboard, once a server has registered
/// them: the account id a login puts in the `me` file beside the log.
/// Until then a human has none, and the log they wrote is their
/// identity.
pub type HumanId = Id<Human>;

/// Marker for `LogId`, the way `Human` pins `HumanId`.
pub enum Log {}

/// Identifies one event log: minted once into the `log-id` file beside
/// it. Before a server registers a human, the log they wrote is their
/// identity.
pub type LogId = Id<Log>;

/// A position in one log: the event with `seq` in the log `log`. `seq`
/// starts at 1 and equals the line number until logs are merged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogCursor {
    pub log: LogId,
    pub seq: u64,
}

/// Token counts for one round trip to the model. `cached_tokens` is
/// `None` when the provider does not report it. Carried by
/// `Payload::ModelCalled`, so it is the core's, not the harness's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: Option<u64>,
}

/// The writer that produced an event - `percept-code`, `percept-cli`,
/// `claude-code` - and where it ran from. `path` is that writer's
/// project root, so two projects using the same tool are still told
/// apart, and a search can filter by `name` alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub path: PathBuf,
}

/// Who an Event is attributed to. Extend by adding a variant. A human
/// carries their id once they have one, so a log shared by more than
/// one person still says which one wrote a given event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Human(Option<HumanId>),
    Agent,
    /// percept itself acting - so far, feeding a tool's output back as
    /// `tool.resulted`.
    System,
}

impl Actor {
    /// The word the log, its search, and a prompt use for this actor.
    pub fn name(self) -> &'static str {
        match self {
            Self::Human(_) => "human",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }
}

/// How much of a long text a reader sees when not shown the whole: a
/// search hit's preview, and a past tool result in the prompt.
pub const PREVIEW_CHARS: usize = 120;

/// Event-specific data. One variant per kind of fact the log records.
/// A variant carries typed fields only when the domain produces or
/// reads them - `message_of` needs `content`, so `MessageReceived` is
/// typed; `App` assembles a thought from streamed text, so
/// `ThoughtRecorded` is; `App` runs the loop that emits `ToolCalled`
/// and feeds `ToolResulted` back, so both are. A `ToolCalled` from
/// another writer has no paired result in this log, but replays as
/// context all the same.
#[derive(Clone)]
pub enum Payload {
    MessageReceived {
        content: String,
    },
    ThoughtRecorded {
        content: String,
    },
    /// A tool call. `arguments` is JSON text the domain routes by
    /// `tool` name but never parses - the tool owns that shape. From
    /// percept's own loop, its `ToolResulted` names this event as
    /// cause; from another writer, no result follows in this log.
    ToolCalled {
        tool: String,
        arguments: String,
    },
    /// What percept fed back for a `ToolCalled` - its output or its
    /// error text. `causation_id` points at the call.
    ToolResulted {
        content: String,
    },
    /// A node added to a cognitive map. `sources` names the events the
    /// node was folded from. `seq` is the node's short id number within
    /// its kind - `d41` is `d` plus this - minted once by `Map::apply`
    /// and carried here so a later fold reads back the same number
    /// rather than recomputing it from its own position, which a
    /// `Scope` can change. `0` on the wire means an event recorded
    /// before short ids existed; `Map::replay` falls back to counting
    /// its position among nodes of its kind for those, so an old log
    /// still folds without a migration.
    NodeAdded {
        map: String,
        node: NodeId,
        kind: String,
        name: String,
        properties: BTreeMap<String, String>,
        sources: Vec<EventId>,
        seq: u32,
    },
    /// A node removed from a cognitive map, with why.
    NodeRemoved {
        map: String,
        node: NodeId,
        reason: String,
        sources: Vec<EventId>,
    },
    /// An edge added to a cognitive map. Carries no id of its own -
    /// `kind`, `from`, and `to` identify one.
    EdgeAdded {
        map: String,
        kind: String,
        from: NodeId,
        to: NodeId,
        sources: Vec<EventId>,
    },
    /// An edge removed from a cognitive map.
    EdgeRemoved {
        map: String,
        kind: String,
        from: NodeId,
        to: NodeId,
        sources: Vec<EventId>,
    },
    /// One round trip to the model - always `System`, never replayed as
    /// dialogue. Caused by the turn's anchor, the same event a thought
    /// or a reply from that round trip is caused by.
    ModelCalled(Usage),
    /// A coding client's session opened against this project - always
    /// `System`. The marker a later `SessionStart` hook call finds to
    /// learn when this source last opened the project here, so it can
    /// cut the log to what changed since then.
    SessionStarted,
    /// A file, or a range of it, as it was when a node cited it -
    /// experience, not judgment: this event says nothing about why the
    /// file was read or what it shows. `path` is repo-relative,
    /// `lines` the 1-based inclusive range read, `None` for the whole
    /// file. `excerpt` is the text read from the tree at publish time,
    /// so the record still reads once the file has moved on. A node
    /// that lists this event's id in its `sources` is what cites it -
    /// the claim lives on the node, never here.
    FileCited {
        path: PathBuf,
        lines: Option<(u32, u32)>,
        excerpt: String,
    },
    /// The human confirming a node's claim - always `Actor::Human`. Named
    /// events, never edges, so a map's fold derives standing from the log
    /// rather than from a mutable graph fact.
    ClaimConfirmed {
        map: String,
        node: NodeId,
    },
    /// The human disputing a node's claim, with why - always
    /// `Actor::Human`.
    ClaimDisputed {
        map: String,
        node: NodeId,
        why: String,
    },
    /// The node ids a human's review showed - always `Actor::Human`. Marks
    /// each `Standing::Seen` until a later `claim.confirmed` or
    /// `claim.disputed` names it.
    ReviewFinished {
        map: String,
        nodes: Vec<NodeId>,
    },
}

/// `path`, with `:from-to` appended for a ranged citation - the
/// label a `file.cited` event reads as wherever it's shown short:
/// the TUI, the context index, and a search preview.
pub fn cited_label(path: &std::path::Path, lines: Option<(u32, u32)>) -> String {
    match lines {
        Some((from, to)) => format!("{}:{from}-{to}", path.display()),
        None => path.display().to_string(),
    }
}

impl Payload {
    /// The event's text - the one string that runs long, and the one a
    /// reader wants to see more of. A tool call carries none: its
    /// `tool` and `arguments` are the model's own short strings. Nor
    /// does a map change: its fields are all short. A `file.cited`
    /// event's `excerpt` is this text - the file's own words, not
    /// judgment - so a search hit inside it carries a match range and a
    /// ranged read reaches it the same way any other long text does.
    pub fn content(&self) -> Option<&str> {
        match self {
            Self::MessageReceived { content }
            | Self::ThoughtRecorded { content }
            | Self::ToolResulted { content } => Some(content),
            Self::FileCited { excerpt, .. } => Some(excerpt),
            Self::ToolCalled { .. }
            | Self::NodeAdded { .. }
            | Self::NodeRemoved { .. }
            | Self::EdgeAdded { .. }
            | Self::EdgeRemoved { .. }
            | Self::ModelCalled(..)
            | Self::SessionStarted
            | Self::ClaimConfirmed { .. }
            | Self::ClaimDisputed { .. }
            | Self::ReviewFinished { .. } => None,
        }
    }
}

/// What kind of fact an Event records - one variant per `Payload`
/// variant, as a value a caller can compare and filter on without
/// matching. The domain's word for what the wire calls `type`; `store`
/// owns the spelling.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    MessageReceived,
    ThoughtRecorded,
    ToolCalled,
    ToolResulted,
    NodeAdded,
    NodeRemoved,
    EdgeAdded,
    EdgeRemoved,
    ModelCalled,
    SessionStarted,
    FileCited,
    ClaimConfirmed,
    ClaimDisputed,
    ReviewFinished,
}

/// One recorded fact in the conversation log. Append-only: a committed
/// Event never changes. `actor` and the `payload` variant together say
/// what happened; `source` says which writer produced it; `causation_id`
/// says what led to it. `Clone` copies a record, id and all - it never
/// makes a committed event editable.
#[derive(Clone)]
pub struct Event {
    id: EventId,
    actor: Actor,
    source: Source,
    causation_id: Option<EventId>,
    created_at: Timestamp,
    payload: Payload,
}

impl Event {
    /// A fresh event. `id` and `created_at` are minted here, so no
    /// caller outside the domain decides what an event's identity is;
    /// the caller owns `source` (which writer produced it) and
    /// `causation_id`.
    pub fn new(
        actor: Actor,
        source: Source,
        causation_id: Option<EventId>,
        payload: Payload,
    ) -> Self {
        Self {
            id: EventId::new(),
            actor,
            source,
            causation_id,
            created_at: Timestamp::now(),
            payload,
        }
    }

    /// A `message.received` event.
    pub fn message_received(
        actor: Actor,
        content: String,
        source: Source,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            actor,
            source,
            causation_id,
            Payload::MessageReceived { content },
        )
    }

    /// A `thought.recorded` event.
    pub fn thought_recorded(
        actor: Actor,
        content: String,
        source: Source,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            actor,
            source,
            causation_id,
            Payload::ThoughtRecorded { content },
        )
    }

    /// A `tool.called` event - always the model's action.
    pub fn tool_called(
        tool: String,
        arguments: String,
        source: Source,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::Agent,
            source,
            causation_id,
            Payload::ToolCalled { tool, arguments },
        )
    }

    /// A `tool.resulted` event - always percept feeding a tool's output
    /// back, never the model.
    pub fn tool_resulted(content: String, source: Source, causation_id: Option<EventId>) -> Self {
        Self::new(
            Actor::System,
            source,
            causation_id,
            Payload::ToolResulted { content },
        )
    }

    /// A `model.called` event - always percept recording what one round
    /// trip to the model cost, never the model's own words.
    pub fn model_called(usage: Usage, source: Source, causation_id: Option<EventId>) -> Self {
        Self::new(
            Actor::System,
            source,
            causation_id,
            Payload::ModelCalled(usage),
        )
    }

    /// A `session.started` event - always percept recording that a
    /// coding client opened a session against this project, never the
    /// model's own words.
    pub fn session_started(source: Source) -> Self {
        Self::new(Actor::System, source, None, Payload::SessionStarted)
    }

    /// A `claim.confirmed` event - always the human's own judgment on a
    /// node's claim, never the model's. `human` is the one who judged
    /// it, from `Jsonl::me`.
    pub fn claim_confirmed(
        map: String,
        node: NodeId,
        human: Option<HumanId>,
        source: Source,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::Human(human),
            source,
            causation_id,
            Payload::ClaimConfirmed { map, node },
        )
    }

    /// A `claim.disputed` event - always the human's own judgment, with
    /// why. `human` is the one who judged it, from `Jsonl::me`.
    pub fn claim_disputed(
        map: String,
        node: NodeId,
        why: String,
        human: Option<HumanId>,
        source: Source,
        causation_id: Option<EventId>,
    ) -> Self {
        Self::new(
            Actor::Human(human),
            source,
            causation_id,
            Payload::ClaimDisputed { map, node, why },
        )
    }

    /// Rebuilds an Event from stored fields - the persistence boundary,
    /// where `id` and `created_at` come from storage rather than being
    /// minted fresh.
    pub fn restore(
        id: EventId,
        actor: Actor,
        source: Source,
        causation_id: Option<EventId>,
        created_at: Timestamp,
        payload: Payload,
    ) -> Self {
        Self {
            id,
            actor,
            source,
            causation_id,
            created_at,
            payload,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }

    pub fn actor(&self) -> Actor {
        self.actor
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn causation_id(&self) -> Option<EventId> {
        self.causation_id
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn payload(&self) -> &Payload {
        &self.payload
    }

    pub fn kind(&self) -> EventKind {
        match self.payload {
            Payload::MessageReceived { .. } => EventKind::MessageReceived,
            Payload::ThoughtRecorded { .. } => EventKind::ThoughtRecorded,
            Payload::ToolCalled { .. } => EventKind::ToolCalled,
            Payload::ToolResulted { .. } => EventKind::ToolResulted,
            Payload::NodeAdded { .. } => EventKind::NodeAdded,
            Payload::NodeRemoved { .. } => EventKind::NodeRemoved,
            Payload::EdgeAdded { .. } => EventKind::EdgeAdded,
            Payload::EdgeRemoved { .. } => EventKind::EdgeRemoved,
            Payload::ModelCalled(..) => EventKind::ModelCalled,
            Payload::SessionStarted => EventKind::SessionStarted,
            Payload::FileCited { .. } => EventKind::FileCited,
            Payload::ClaimConfirmed { .. } => EventKind::ClaimConfirmed,
            Payload::ClaimDisputed { .. } => EventKind::ClaimDisputed,
            Payload::ReviewFinished { .. } => EventKind::ReviewFinished,
        }
    }
}
