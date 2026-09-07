# The harness

A proposal, written 2026-09-07. It names what percept already builds
around a model, splits it into parts that vary for different reasons,
and puts the context builder where it can be changed without touching
the loop. Nothing here changes behaviour on its own; it gives the
window work a home.

## Terms

An agent is three things, and percept owns two of them.

| Term | What it is | In percept |
|---|---|---|
| Model | A reply given messages. | The `Model` port; the picker swaps it. |
| Memory | What the agent knows across sessions and models, with the operations on it. | The event log and the maps folded from it, behind `EventLog`, `EventSearch`, `MapReader`, and `MapRenderer`. |
| Harness | Everything between the two: the loop, the view, and what the model may reach for. | Today, all of `App` plus the wiring in `main`. |

Agent = model + harness + memory. The memory is separate from the
harness because it is the one part every harness and every model
shares. A coding session and a reflect session over one log are the
same agent with a different harness. Memory knows nothing of a model:
its ports append, load, search, fold, and render, and `store`
implements them.

The harness itself has three parts. They vary on different axes, which
is why they are three and not one.

| Part | Question it answers | Varies with |
|---|---|---|
| Control plane | What happens next: ask the model, run a tool, ask the user, end the turn, undo. | Nothing. One loop for every purpose. |
| Context | What the model sees this round. | The purpose, and the model's size. |
| Capabilities | What the model may do, what it is told, and what guards it. | The purpose. |

```
                tui · cli
                    │ drive
                    ▼
    ┌───────────────────────────────────────┐
    │ Harness                               │
    │                                       │
    │   control plane      the loop (App)   │
    │        │ asks: what does it see now?  │
    │        ▼                              │
    │   context            a list of typed  │
    │        │             sections         │
    │        │ renders from memory, carries │
    │        ▼                              │
    │   capabilities       tools · policy · │
    │                      cap · snapshot · │
    │                      instructions     │
    └───────────────────────────────────────┘
       reply │                   │ commit, fold, render
             ▼                   ▼
           Model               Memory
     Ollama · OpenAI ·      log · maps ·
        Fireworks            .percept/
```

### Memory is read two ways, and both are harness

The context pushes a view: it folds the maps and windows the log
before the model asks. The tools let the model pull: `search_events`,
`read_event`, `read_map`, and `revise_map` take a JSON argument the
model can write and call a memory port. Which adapters a harness
offers is the harness's choice; the memory underneath is the same. The
analogy is a database and one client's SDK over it.

Memory has one writer. A tool never appends to the log: `revise_map`
checks its change against the log and hands the payloads back, and
the loop commits them, caused by the call. Every event goes through
`App::commit`, whoever asked for it.

## Where the parts are today

| Part | Today | Proposed |
|---|---|---|
| Control plane | `App`: `submit`, `begin_tool`, `finish_tool`, `decline_tool`, `end_stream`, `undo`. | Unchanged. `App` is the loop and nothing else. |
| Context | `build_request` and `window_start`, private in `App`; `CONTEXT_EVENTS` a constant; `MapShape` a field. | A `Context` value on the harness: a list of sections, one render function. |
| Capabilities | `tools`, `policy`, `tool_cap`, `snapshot`, `instructions` as `App` fields, set by `with_*` builders. | Fields of a `Harness` value. |
| Wiring | `Toolset` in `main`, from `PERCEPT_TOOLS`; `PERCEPT_MAPS` sets the map shape. | Unchanged. Both variables set knobs on the one harness. |

The bundle `main` builds is already a harness: the file tools, the
ask-before-writes policy, a cap of fifty, a git snapshot, and the
checkout's `AGENTS.md`, or the log tools alone where no checkout
exists. It has no name, and its view is hardwired into the loop. That
is what makes the view hard to change and impossible to vary.

## The harness value

```rust
pub struct Harness {
    pub tools: Vec<Arc<dyn Tool>>,
    pub policy: Arc<dyn Policy>,
    pub tool_cap: usize,
    pub snapshot: Option<Arc<dyn Snapshot>>,
    /// The project's own instructions, as its AGENTS.md has them.
    /// Read once at startup by the wiring that knows the checkout.
    pub instructions: Option<String>,
    pub context: Context,
}
```

`App::new` takes a `Harness` in place of the tools, the map shape, and
the four `with_*` builders. The ports stay where they are in the
domain; the struct only groups them. It lives in `app`, since it
composes domain objects and adds no vocabulary of its own.

The instructions text sits here and not on the context. It is content
the model is given, chosen per purpose, like the tools. The context
holds only shapes: how a thing is shown, never the thing.

## The context

The context is a list of typed sections. Each section is one part of
the request and carries its own shape, the way `MapShape` does now.
Order and membership are data, so a new purpose is a new list, not a
new function.

```rust
/// Listed stable first, the order the list below uses. The variant
/// order itself means nothing; the list's does.
pub enum Section {
    /// The harness's instructions as system text.
    Instructions,
    /// Every map, each as its schema's purpose line and this shape.
    Maps(MapShape),
    /// The transcript back as far as the window reaches, its tool
    /// results cut to a head, plus the whole turn in progress. Before
    /// it, one line per event for `index` events further back, so the
    /// model can open with `read_event` what it can no longer see.
    History { window: Window, index: usize },
    /// The current time, for `since` on the tools. It changes every
    /// round, so it goes after everything a provider could cache.
    Time,
}

pub struct Context {
    pub sections: Vec<Section>,
}

/// The one harness's view, as `Harness::new` builds it.
Context {
    sections: vec![
        Section::Instructions,
        Section::Maps(map_shape),
        Section::History {
            window: Window { share: 0.125, keep: 0.0625, floor: 8_000 },
            index: 200,
        },
        Section::Time,
    ],
}

/// A share of the model's context window. History fills to `share`,
/// is cut back to `keep`, then holds still until it fills again. For
/// a model that reports no window, `floor` is the high mark and half
/// of it the low.
pub struct Window { pub share: f32, pub keep: f32, pub floor: u32 }
```

Three things in the first draft turned out to be one section. The
index starts where history stops and needs the same cut, so it is a
field of `History`. A tool result outside the current turn is always
cut to its head with a `read_event` handle, so there is no shape to
choose. And a window counted in events had no caller once the token
window existed, so `Window` is a struct, not an enum.

Each section renders itself from a `View`: the transcript, the scope
the maps fold in, the turn in progress, the model's context window,
and what `App` decided about the tools. One function walks the list:

```rust
impl Context {
    pub fn build(&self, view: View) -> Result<ModelRequest, Box<dyn Error>>
}
```

The window's start is a function of the log alone: the cuts are
replayed over the whole transcript on every build, so two rounds agree
on the start with no state kept between them, and a restart lands on
the same cut. Tokens are estimated at four characters each, over what
`to_messages` would carry.

### Two orders

The list has one order for the budget and another for the messages,
and the builder keeps them apart.

- **Budget priority** says who gives way when the request is too big:
  instructions, then maps, then the current turn, then history, then
  the index. Today only history gives way, to its window; the maps
  are unconditional and the index is a fixed count of lines. Should
  the maps ever need to give way too, this is the order.
- **Message sequence** is the order the model reads. It is set by what
  changes least often, for the cache.

### Built for the cache

Every provider percept has - OpenAI, Ollama, Fireworks - reuses a
request's prefix when it matches the last request's. Within a tool
loop each round appends one call and one result at the tail, so
everything before them is reusable. Today the first message is the
current time, which changes every round, and the prefix dies at
message one. A fifty-call coding turn pays for the instructions, the
maps, and the whole history fifty times. `Usage` already records
`cached_tokens`, so the gain is measurable.

The message sequence, by how often each section changes:

| Section | Changes when | Position |
|---|---|---|
| Instructions | Never within a session. | First |
| Tool specs | Never, until the cap drops them at a turn's end. | With the prefix, where the provider puts them |
| Maps | A `revise_map` commits. A fold, so identical between changes. | Second |
| Index | Grows at its tail as events leave the window. | Third |
| History | Grows at its tail every round; its start moves when the window slides. | Fourth |
| Time, budget note | Every round. | Last |

Two rules follow from it:

- **The window slides in steps, not per event.** A window that drops
  the oldest message every round moves the prefix every round. So
  history fills to `share`, cuts back to `keep`, and holds still until
  it fills again. A cut lands on a user prompt, never inside a tool
  round. The boundary between the index and full history moves with
  it, in the same steps.
- **A map's stability rule is also a cache rule.** Add beside what a
  reader has seen and never move it, which AGENTS.md asks for the
  reader's sake, is what keeps a rendered map a prefix of its next
  render. Nodes fold in log order, so a new node lands at the end.

Ordering stable first is all automatic prefix caching needs. A
provider that wants an explicit boundary, as Anthropic's does, would
mark the point between the last stable section and the first volatile
one. Typed sections give it that point; a flat string would not.

The system message a model reads first is the one it weighs most,
which is why the instructions lead today. Stable-first ordering agrees
with that. Only the time moves to the end, and the time is a fact, not
an instruction.

### What the context does not decide

- **The tool cap.** `App` counts calls and knows when the turn is at
  its cap. It tells the context, which adds the budget-spent line and
  sends no tools. The context never counts.
- **Commit order and causation.** Every event still goes through
  `App::commit`, thought before reply before usage, each caused by the
  turn's anchor.
- **Undo.** The snapshot is taken by the loop before the prompt is on
  the record, as now.

### What stays true of the view

- The maps sit outside the window, so the model always sees what it
  built.
- The current turn is never cut. A long tool loop can never evict the
  question it is answering.
- Percept's own prompts, a `reflect`, are dropped from history so they
  never stand as an instruction in a later turn.

### Not a template

A text template with injected variables is the same idea one step
further, and it waits. A variable in a template is a section rendered
to text, so the sections come first either way. Two things count
against a template now: the request is a list of messages with roles,
and a provider may want the stable sections in a system block, which
one flattened string throws away; and percept's system texts are short
sentences, with the maps rendering themselves through `Display`, so
there is little for a variable to substitute. When a harness is
authored from a file, a section list in TOML with a template beside it
is the natural form, and its variables are the sections' renders. That
is a later issue with its own decisions.

## One harness today

A harness is for a purpose. Percept has one purpose today: a
conversation over the log that may also change the tree. The `maps`
and `code` toolsets are not two harnesses; they are that one harness
with the tree on or off, and whether the tree is on is a fact of the
environment, a git checkout or not. So `main` builds one `Harness`,
and `PERCEPT_TOOLS` and `PERCEPT_MAPS` stay what they are: knobs on
it.

| Tree | Tools | Policy | Cap | Snapshot | Instructions |
|---|---|---|---|---|---|
| off | the four log and map tools | allow all | 5 | none | none |
| on | the four, plus the seven file tools | ask before writes and bash | 50 | git | `AGENTS.md` |

The context is the same list either way.

A second harness appears when a second purpose needs a different
view, not a different toolset. Two candidates, neither part of this
change:

| Purpose | What would differ |
|---|---|
| reflect: revise the maps from the log at a session's end | No tree; maps whole; a long index so the whole session is one `read_event` away. |
| ask: one headless turn | The least view that answers; no policy prompts, since no one is there to answer them. |

Until one of those differs in its section list, it is the same
harness driven differently, and it stays that way.

## Seeing the context

A `/context` command in the TUI shows what the model was shown. It is
a fold over the section list: each section's name, shape, message
count, and tokens by the builder's own estimate, so there is one
number and not two that drift. Beside them, the last round's
`input_tokens` and `cached_tokens`, which percept already records.

It is the verification budget applied to the harness. Two claims in
this design cannot be checked without it: that the window reached what
the model needed, and that the cache held. The model has `read_map`
and `search_events` to see what it holds; this is the human's way to
see what the model saw. Read-only, printed into the transcript the way
`/models` is. A `percept context` subcommand can share the render if a
headless run ever needs it.

It comes last in the window step, not first: before the sections exist
it would be a second hand-written description of `build_request`, and
it would drift.

## Decisions the user lives with

These are settled before the build, not assumed.

1. The `Tokens` window's defaults: share, keep, and floor. Settled:
   one eighth of the model's window, cut back to one sixteenth, and a
   floor of eight thousand tokens.
2. The index bound. Two hundred events is a page the model can scan.
3. The command's name. `/context`, since both coding clients in use
   call it that.

## Steps

Each landed as one commit on `feat/harness`.

1. **Extract.** `Harness`, `Context`, and `Section` in `app`;
   `build_request` and `window_start` became `Context::build`; `App::new`
   takes a `Harness`. Behaviour identical, the existing tests unchanged.
2. **Reorder for the cache.** Time to the end, with a test that a tool
   round only appends to the request.
3. **Vary the view.** The token window with its step rule, the cut of
   old tool results, and the index, each with its tests.
4. **`/context`.** The render over the sections.

## Alternatives weighed

- **`maps` and `code` as two named harnesses, picked by a variable.**
  They differ by whether a checkout exists, which is the environment
  and not a purpose. Naming them would add a variable that selects
  between two settings of one thing.
- **A `ContextBuilder` trait with one implementation per harness.**
  There is one harness. A trait earns its place when a purpose needs
  code instead of a value, or a test needs a fake. Neither is the case
  today.
- **A flat struct of five shape fields.** Order and membership were
  hidden in the build function, and the budget rule with them. A list
  of typed sections makes both visible, and gives a provider a
  boundary to mark for its cache.
- **A window counted in events, a result shape, and an index section
  as separate knobs.** Each had one value in use. The event window had
  no caller once the token window existed; a result outside the turn
  is always cut; the index belongs to the history it precedes.
- **The store's JSON summary line for the cut result and the index.**
  It is the serde boundary, and `app` may not depend on `store`. The
  builder prints its own one-line text preview with the id, which is
  what the model needs to call `read_event`.
- **The context's parts as policy objects.** The word `Policy` already
  means whether a tool call runs or asks. A second family under the
  same word makes a reader ask which kind every time. The parts cut a
  whole to a fragment, as `Selection` does to a map, so they are
  shapes.
- **Instructions as a field of the context.** The other fields are
  shapes; this one held content, read from the tree at startup. It
  moved to the harness, beside the tools, where the wiring that read
  it puts it.
- **More fields on `App` for the window work.** It is how the view got
  hardwired into the loop in the first place; a fourth `with_*` builder
  makes the next change as hard as this one.
- **Raise `CONTEXT_EVENTS`.** Fights the design that says a model
  searches what it cannot hold, and still misses a plan written before
  a long discussion. The token window reaches it because it scales with
  the model.
