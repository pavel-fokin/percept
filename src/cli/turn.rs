use std::io::{self, Write};

use clap::Args;
use tokio_stream::StreamExt;

use super::{non_blank, stop_if_pipe_closed};
use crate::app::{run_tool, AppService, ToolStep};
use crate::core::Actor;
use crate::harness::Chunk;

#[cfg(test)]
mod tests;

#[derive(Args)]
pub struct AskArgs {
    /// The prompt to send.
    #[arg(value_parser = non_blank)]
    pub prompt: String,
    /// Run every tool call the policy would ask about. Headless, there
    /// is no one to ask, so without this such a call is declined.
    #[arg(long)]
    pub yes: bool,
}

/// Runs one turn on `app` - submitting `prompt` as `actor`, then
/// draining the reply stream chunk by chunk - and prints the reply to
/// stdout. No channel, no spawned task: unlike the TUI, nothing else
/// needs the thread while headless, so a tool runs inline and the turn
/// is one plain `await` loop. Each tool call and its result print to
/// stderr as they happen, so stdout stays pipeable. That trace is for
/// watching a run live; the log is what a run is read back from. `yes`
/// is the user's standing answer to every call the policy puts to
/// them - what `y` is in the TUI; without it, headless, such a call is
/// declined.
pub async fn run_turn(
    mut app: Box<dyn AppService>,
    actor: Actor,
    prompt: String,
    yes: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = app.submit_as(actor, prompt)?;
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
                    ToolStep::Ask(_, arguments) if !yes => {
                        eprintln!("⚒ {tool}({arguments}) - declined: needs approval, run in the TUI or pass --yes");
                        app.decline_tool()?
                    }
                    ToolStep::Run(run, arguments) | ToolStep::Ask(run, arguments) => {
                        eprintln!("⚒ {tool}({arguments})");
                        let output = run_tool(&*run, &arguments);
                        eprintln!("⚒ {}", output.content);
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
