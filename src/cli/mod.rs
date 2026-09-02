//! The command-line surface: `percept events publish` appends one event
//! without opening the TUI, `percept events search` queries the log,
//! `percept events show` dereferences one event by id, and `percept ask`
//! runs one full turn - including the tool loop - and prints the reply.
//! A presentation-layer peer of `tui` - it forwards parsed input to
//! `store` and `app`, and has no chat logic of its own: `ask` drives the
//! same `AppService` turn policy `tui` does, just inline instead of over
//! a channel.
//!
//! `search` and `show` are the query primitive a model composes with:
//! every line is JSONL, for a caller piping into `jq`, never a table or
//! prose. `search`'s default line shortens long strings in the payload,
//! so a caller spends tokens on the whole of one deliberately, via
//! `--full`, `show`, or `show --range` into one `content`.

use std::io::{self, Write};

use clap::{Args, Parser, Subcommand};
use tokio_stream::StreamExt;

use crate::app::{run_tool, AppService, ToolStep};
use crate::percept::{Chunk, EventId, EventLog, EventQuery, EventSearch};
use crate::shared::Timestamp;
use crate::store;

#[derive(Parser)]
#[command(name = "percept")]
#[command(about = "Record what happens across your tools, so a model can query it.")]
#[command(long_about = "\
Record what happens across your tools, so a model can query it.

percept keeps an append-only log of events - prompts, replies, and tool \
calls - in percept.jsonl in the working directory. It never ranks, \
summarises, or answers: its job is to make looking cheap and leave \
relevance to the caller.

Run with no arguments to open the TUI. Every subcommand reaches the log \
without it: `events publish` appends one event, `events search` and \
`events show` query it, and `ask` runs one full turn and prints the reply.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Work with the event log directly, bypassing the TUI.
    Events {
        #[command(subcommand)]
        command: EventsCommand,
    },
    /// Run one turn headlessly and print the reply.
    Ask(AskArgs),
}

#[derive(Subcommand)]
pub enum EventsCommand {
    /// Append one event to the log.
    Publish(PublishArgs),
    /// Search events, one JSON object per line, oldest first.
    Search(SearchArgs),
    /// Print one event by id.
    Show(ShowArgs),
}

#[derive(Args)]
pub struct PublishArgs {
    #[arg(long)]
    actor: String,
    #[arg(long, value_parser = non_blank)]
    source: String,
    #[arg(long = "type")]
    kind: String,
    #[arg(long)]
    payload: String,
}

#[derive(Args, Default)]
#[command(after_help = "\
Examples:
  # The 20 most recent events, printed oldest first
  percept events search --size 20

  # What the model did in the last day
  percept events search --since 1d --type tool.called

  # Prompts and replies naming a deploy, 300 characters around each hit
  percept events search --contains deploy --type message.received --preview 300

  # The same, with full payloads
  percept events search --contains deploy --type message.received --full

  # Two windows that tile with no gap or overlap
  percept events search --since 2d --until 1d
  percept events search --since 1d")]
pub struct SearchArgs {
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Inclusive.
    #[arg(long)]
    since: Option<String>,
    /// An ISO-8601 timestamp, or a relative shorthand measured back
    /// from now: `<N>d`, `<N>h`, `<N>m`. Exclusive.
    #[arg(long)]
    until: Option<String>,
    /// Repeatable. An event matching any of these sources passes.
    #[arg(long)]
    source: Vec<String>,
    /// Repeatable. An event matching any of these actors passes.
    #[arg(long)]
    actor: Vec<String>,
    /// Repeatable. An event matching any of these types passes.
    #[arg(long = "type")]
    kind: Vec<String>,
    /// Repeatable. An event whose payload carries any of these
    /// substrings, case-insensitively, passes.
    #[arg(long, value_parser = non_blank)]
    contains: Vec<String>,
    /// Keep only the N most recent matching events. Output still runs
    /// oldest first.
    #[arg(long)]
    size: Option<usize>,
    /// How many characters of `content` a line keeps, cut around the
    /// first `--contains` hit when there is one.
    #[arg(long, default_value_t = store::PREVIEW_CHARS, value_parser = at_least_one)]
    preview: usize,
    /// Print the whole wire event per line instead of the constant-size
    /// default.
    #[arg(long)]
    full: bool,
}

#[derive(Args)]
pub struct ShowArgs {
    id: String,
    /// A character range `START:END` into `payload.content`, `END`
    /// exclusive; omit `END` to reach the end of `content`, e.g.
    /// `400:`. Only event kinds that carry `content` support a range.
    #[arg(long, value_parser = parse_range)]
    range: Option<(usize, Option<usize>)>,
}

#[derive(Args)]
pub struct AskArgs {
    /// The prompt to send.
    #[arg(value_parser = non_blank)]
    prompt: String,
}

/// Parses `--range START:END`: `END` may be blank, reaching the end of
/// `content`.
fn parse_range(s: &str) -> Result<(usize, Option<usize>), String> {
    let (start, end) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid range {s:?}, expected START:END"))?;
    let start: usize = start
        .parse()
        .map_err(|_| format!("invalid range start {start:?}"))?;
    let end = if end.is_empty() {
        None
    } else {
        Some(
            end.parse()
                .map_err(|_| format!("invalid range end {end:?}"))?,
        )
    };
    Ok((start, end))
}

/// A window of zero characters shows nothing and reads as a mistake.
fn at_least_one(s: &str) -> Result<usize, String> {
    match s.parse::<usize>() {
        Ok(0) => Err("must be at least 1".to_string()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
}

/// Rejects a blank value at parse time. A source that names nobody, a
/// search term contained by everything, a prompt that asks nothing -
/// each looks deliberate to a reader while meaning nothing.
fn non_blank(s: &str) -> Result<String, String> {
    if s.trim().is_empty() {
        return Err("must not be blank".to_string());
    }
    Ok(s.to_string())
}

/// Appends one event built from `args` to `log`. `store` owns the
/// decode, so the CLI only parses flags.
pub fn publish(args: PublishArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let payload = serde_json::from_str(&args.payload).map_err(store::Error::BadPayload)?;
    let event = store::decode(&args.actor, args.source, &args.kind, payload)?;
    log.append(&event)
}

/// Searches `log` for events matching `args`, printing one JSON object
/// per line in log order. `store` owns the wire shape; the CLI only
/// builds the query and formats the result.
pub fn search(args: SearchArgs, log: &dyn EventSearch) -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query(&args)?;
    let events = log.search(&query)?;

    // One buffered writer rather than a syscall per line: the caller is
    // a pipe into jq, and a whole-log listing runs to thousands of
    // lines.
    let mut out = io::BufWriter::new(io::stdout().lock());
    for event in &events {
        let line = if args.full {
            store::encode(event)
        } else {
            store::summarize(event, query.hit(event), args.preview)
        };
        if let Err(e) = writeln!(out, "{line}") {
            return stop_if_pipe_closed(e);
        }
    }
    out.flush().or_else(stop_if_pipe_closed)
}

/// A reader that stops early - `head`, or a `jq` that has seen enough -
/// closes the pipe. That is the caller's choice, not a failure to
/// report.
fn stop_if_pipe_closed(e: io::Error) -> Result<(), Box<dyn std::error::Error>> {
    if e.kind() == io::ErrorKind::BrokenPipe {
        Ok(())
    } else {
        Err(e.into())
    }
}

/// The `EventQuery` `args` asks for, with every flag parsed once into
/// the value it is compared against. A filter naming something the log
/// has no word for is an error here rather than a query that quietly
/// matches nothing.
fn parse_query(args: &SearchArgs) -> Result<EventQuery, String> {
    let kinds = args
        .kind
        .iter()
        .map(|kind| store::parse_kind(kind).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let actors = args
        .actor
        .iter()
        .map(|actor| store::parse_actor(actor).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;

    let since = args
        .since
        .as_deref()
        .map(|s| parse_time("since", s))
        .transpose()?;
    let until = args
        .until
        .as_deref()
        .map(|s| parse_time("until", s))
        .transpose()?;

    // An inverted window can never match, whatever the log holds -
    // `--since 1h --until 2h` is how "between one and two hours ago"
    // is mistyped. Rejecting it keeps an empty result meaning the log
    // has nothing, the same guarantee the filters above give.
    if let (Some(since), Some(until)) = (since, until) {
        if since >= until {
            return Err(format!("--since {since} is not before --until {until}"));
        }
    }

    Ok(EventQuery {
        since,
        until,
        actors,
        sources: args.source.clone(),
        kinds,
        text: args.contains.clone(),
        size: args.size,
    })
}

/// Prints the one event `args.id` names. An id the log doesn't carry
/// fails loudly rather than printing nothing, so an empty result never
/// means "your id was wrong". With `--range`, prints `payload.content`
/// sliced to it instead of the whole event.
pub fn show(args: ShowArgs, log: &dyn EventLog) -> Result<(), Box<dyn std::error::Error>> {
    let wanted: EventId = store::parse_event_id(&args.id)?;
    let event = log
        .get(wanted)?
        .ok_or_else(|| format!("no event with id {}", args.id))?;

    let line = match args.range {
        Some((start, end)) => store::excerpt(&event, Some(start), end)?,
        None => store::encode(&event),
    };
    println!("{line}");
    Ok(())
}

/// Runs one turn on `app` - submitting `prompt`, then draining the reply
/// stream chunk by chunk - and prints the reply to stdout. No channel,
/// no spawned task: unlike the TUI, nothing else needs the thread while
/// headless, so a tool runs inline and the turn is one plain `await`
/// loop. Each tool call and its result print to stderr as they happen,
/// so stdout stays pipeable. That trace is for watching a run live; the
/// log is what a run is read back from.
pub async fn ask(
    args: AskArgs,
    mut app: Box<dyn AppService>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = app.submit(args.prompt)?;
    // What stdout gets. `App` clears its own reply buffer at each tool
    // call and again when the cap ends a turn, so a turn that spoke
    // before calling a tool would otherwise print only its last leg.
    let mut reply = String::new();

    loop {
        match stream.next().await {
            // Each arm echoes for itself: `App` decides what a call
            // means, and a call it refused never happened.
            Some(Ok(Chunk::ToolCall { tool, arguments })) => {
                stream = match app.begin_tool(&tool, arguments.clone())? {
                    ToolStep::Run(run, arguments) => {
                        eprintln!("⚒ {tool}({arguments})");
                        let output = run_tool(&*run, &arguments);
                        eprintln!("⚒ {output}");
                        app.finish_tool(output)?
                    }
                    ToolStep::Continue(stream) => {
                        eprintln!("⚒ {tool}({arguments}) - no such tool");
                        stream
                    }
                    ToolStep::Stop => break,
                };
            }
            Some(Ok(chunk)) => {
                if let Chunk::Reply(text) = &chunk {
                    reply.push_str(text);
                }
                app.append_chunk(chunk);
            }
            // A failed reply is shown, never logged - the stream's own
            // words are this run's error. Whatever text arrived before
            // it still commits, and still prints: the words reached the
            // log, so stdout is not the surface that should lose them.
            Some(Err(err)) => {
                app.end_stream()?;
                print_reply(&reply)?;
                return Err(err.to_string().into());
            }
            None => break,
        }
    }

    app.end_stream()?;
    print_reply(&reply)
}

/// Writes the reply to stdout, saying nothing when the turn produced no
/// text. A reader that stops early is the caller's choice, not a
/// failure - the same courtesy `search` extends.
fn print_reply(reply: &str) -> Result<(), Box<dyn std::error::Error>> {
    if reply.is_empty() {
        return Ok(());
    }
    let mut out = io::stdout().lock();
    writeln!(out, "{reply}").or_else(stop_if_pipe_closed)
}

/// Parses a `--since`/`--until` value: an ISO-8601 timestamp, or a
/// relative shorthand - `<N>d`, `<N>h`, `<N>m` - measured back from now.
/// `flag` names the flag the value came from, so a rejected value's
/// error says which one.
fn parse_time(flag: &str, s: &str) -> Result<Timestamp, String> {
    let parsed = match relative_minutes(s) {
        Some(minutes) => Timestamp::now().minus_minutes(minutes),
        None => s.parse().ok(),
    };
    parsed.ok_or_else(|| format!("invalid --{flag} value {s}"))
}

/// `<N>d`, `<N>h`, or `<N>m` as a count of minutes. `None` for anything
/// else - `parse_time` then tries it as ISO-8601.
fn relative_minutes(s: &str) -> Option<i64> {
    let (digits, unit) = s.split_at_checked(s.len().checked_sub(1)?)?;
    let n: i64 = digits.parse().ok()?;

    match unit {
        "d" => n.checked_mul(24 * 60),
        "h" => n.checked_mul(60),
        "m" => Some(n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::percept::{self, Payload};
    use crate::testing::{content, FakeLog, FakeTool, Scripted};
    use std::sync::Arc;

    fn args(actor: &str, payload: &str) -> PublishArgs {
        PublishArgs {
            actor: actor.to_string(),
            source: "claude-code".to_string(),
            kind: "message.received".to_string(),
            payload: payload.to_string(),
        }
    }

    #[test]
    fn a_valid_publish_appends_one_event_carrying_its_source() {
        let log = FakeLog::default();
        publish(args("user", r#"{"content":"hi"}"#), &log).unwrap();

        let events = log.load().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source(), "claude-code");
        assert!(events[0].actor() == percept::Actor::User);
    }

    #[test]
    fn a_payload_field_the_type_does_not_record_is_rejected() {
        let log = FakeLog::default();
        let extra = r#"{"content":"hi","meta":{"thread":42}}"#;
        assert!(publish(args("user", extra), &log).is_err());
        assert!(log.load().unwrap().is_empty());
    }

    #[test]
    fn a_rejected_event_appends_nothing() {
        let log = FakeLog::default();
        assert!(publish(args("robot", r#"{"content":"hi"}"#), &log).is_err());
        assert!(publish(args("user", "not json"), &log).is_err());
        assert!(log.load().unwrap().is_empty());
    }

    #[test]
    fn parse_time_parses_an_iso8601_timestamp() {
        let parsed = parse_time("since", "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(parsed.to_string(), "2026-01-01T00:00:00Z");
    }

    #[test]
    fn parse_time_parses_relative_shorthand_as_a_time_in_the_past() {
        let now = Timestamp::now();
        for shorthand in ["1d", "2h", "30m"] {
            let parsed = parse_time("until", shorthand).unwrap();
            assert!(parsed < now, "{shorthand} should parse to before now");
        }
    }

    #[test]
    fn parse_time_rejects_an_unparseable_value_and_names_its_flag() {
        let err = parse_time("until", "3x").err().unwrap();
        assert_eq!(err, "invalid --until value 3x");
    }

    #[test]
    fn an_unknown_type_filter_is_rejected_rather_than_matching_nothing() {
        let args = SearchArgs {
            kind: vec!["message.recieved".to_string()],
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }

    #[test]
    fn every_flag_reaches_the_query_it_builds() {
        let args = SearchArgs {
            source: vec!["tui".to_string(), "cli".to_string()],
            actor: vec!["user".to_string()],
            kind: vec!["tool.called".to_string()],
            contains: vec!["deploy".to_string()],
            size: Some(3),
            since: Some("1d".to_string()),
            ..Default::default()
        };

        let query = parse_query(&args).unwrap();

        assert_eq!(query.sources, vec!["tui", "cli"]);
        assert!(query.actors == vec![percept::Actor::User]);
        assert!(query.kinds == vec![percept::EventKind::ToolCalled]);
        assert_eq!(query.text, vec!["deploy".to_string()]);
        assert_eq!(query.size, Some(3));
        assert!(query.since.is_some() && query.until.is_none());
    }

    #[test]
    fn a_window_that_ends_before_it_starts_is_rejected() {
        let args = SearchArgs {
            since: Some("1h".to_string()),
            until: Some("2h".to_string()),
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }

    #[test]
    fn an_unknown_actor_filter_is_rejected_rather_than_matching_nothing() {
        let args = SearchArgs {
            actor: vec!["User".to_string()],
            ..Default::default()
        };
        assert!(parse_query(&args).is_err());
    }

    #[test]
    fn a_blank_contains_value_is_rejected_at_parse() {
        let ok = Cli::try_parse_from(["percept", "events", "search", "--contains", "deploy"]);
        assert!(ok.is_ok());

        let blank = Cli::try_parse_from(["percept", "events", "search", "--contains", " "]);
        assert!(blank.is_err());
    }

    #[test]
    fn a_zero_preview_is_rejected_at_parse() {
        let zero = Cli::try_parse_from(["percept", "events", "search", "--preview", "0"]);
        assert!(zero.is_err());
        let ok = Cli::try_parse_from(["percept", "events", "search", "--preview", "300"]);
        assert!(ok.is_ok());
    }

    #[test]
    fn a_range_without_an_end_reaches_the_end_of_content() {
        let ok = Cli::try_parse_from(["percept", "events", "show", "abc", "--range", "400:"]);
        assert!(ok.is_ok());
    }

    #[test]
    fn a_range_with_no_colon_is_rejected_at_parse() {
        let bad = Cli::try_parse_from(["percept", "events", "show", "abc", "--range", "400"]);
        assert!(bad.is_err());
    }

    fn ask_args(prompt: &str) -> AskArgs {
        AskArgs {
            prompt: prompt.to_string(),
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ask_runs_one_tool_round_and_commits_the_final_reply() {
        let model = Scripted::new(
            vec![
                vec![percept::Chunk::ToolCall {
                    tool: "search_events".to_string(),
                    arguments: "{}".to_string(),
                }],
                vec![percept::Chunk::Reply("found it".to_string())],
            ],
            true,
        );
        let log = Arc::new(FakeLog::default());
        let tools: Vec<Arc<dyn percept::Tool>> = vec![Arc::new(FakeTool)];
        let app = App::new(Arc::new(model), log.clone(), tools, "cli".to_string()).unwrap();

        ask(ask_args("what happened"), Box::new(app)).await.unwrap();

        let events = log.load().unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].source(), "cli");
        assert!(matches!(
            events[1].payload(),
            Payload::ToolCalled { tool, .. } if tool == "search_events"
        ));
        assert!(matches!(
            events[2].payload(),
            Payload::ToolResulted { content } if content == "ran"
        ));
        assert_eq!(content(&events[3]), "found it");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_stream_error_ends_the_turn_but_still_commits_partial_text() {
        let log = Arc::new(FakeLog::default());
        // A reply that breaks mid-stream, after saying something.
        let model = Scripted::failing(
            vec![vec![
                Ok(percept::Chunk::Reply("partial".to_string())),
                Err("connection dropped".into()),
            ]],
            false,
        );
        let app = App::new(Arc::new(model), log.clone(), Vec::new(), "cli".to_string()).unwrap();

        let result = ask(ask_args("hi"), Box::new(app)).await;

        assert!(result.is_err());
        let events = log.load().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(content(&events[1]), "partial");
    }
}
