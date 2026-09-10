use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::harness::{Tool, ToolOutput, ToolSpec};
use crate::workspace::Workspace;

/// How long a call runs when the model gives no `timeout_secs`.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// The longest a call may run, regardless of what the model asks for.
const MAX_TIMEOUT_SECS: u64 = 600;

/// Output past this many characters is cut, with a trailer saying so.
const OUTPUT_LIMIT: usize = 30_000;

/// How often the running command is checked against its deadline.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// The `bash` tool: runs a command with `bash -c` at the workspace root
/// and reports its exit code, stdout, and stderr. Bash, not `sh`,
/// because the name promises it and a model writes bashisms. Not
/// interactive - a command that waits on stdin hangs until it times
/// out. The command gets a process group of its own, so a timeout
/// kills whatever it started, not only the shell.
pub struct Bash {
    workspace: Arc<Workspace>,
}

impl Bash {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "bash";

const DESCRIPTION: &str = "Run a command at the workspace root with \
    `bash -c`. Not interactive - a command that waits on stdin hangs \
    until it times out, and a background process it leaves behind is \
    killed at the timeout. Output is truncated past 30000 characters. \
    The first line of the result is `exit {code}` (or `killed by \
    signal`), followed by stdout, and then stderr under a `--- stderr \
    ---` marker when there is any.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "command": {"type": "string", "description": "the shell command to run"},
    "timeout_secs": {"type": "integer", "minimum": 1, "description": "how long to wait before killing the command; defaults to 120, capped at 600"}
  },
  "required": ["command"],
  "additionalProperties": false
}"#;

/// `description` is accepted and ignored: a model trained on another
/// agent's tool of the same name sends one with every call, and
/// refusing it cost nine retries in a row before the model gave up.
/// It stays out of the schema, so a model that has not learned the
/// habit is not taught it.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    command: String,
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_SECS
}

impl Tool for Bash {
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: NAME,
            description: DESCRIPTION,
            parameters: PARAMETERS,
        }
    }

    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>> {
        let args: Args = serde_json::from_str(arguments)?;
        let timeout = Duration::from_secs(args.timeout_secs.min(MAX_TIMEOUT_SECS));

        let mut child = Command::new("bash")
            .arg("-c")
            .arg(&args.command)
            .current_dir(self.workspace.root())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;

        let stdout_thread = drain(child.stdout.take().expect("stdout was piped"));
        let stderr_thread = drain(child.stderr.take().expect("stderr was piped"));

        // Done means the shell has exited and both pipes have closed: a
        // background process the shell left holding a pipe keeps the
        // call open, and the deadline is what ends it.
        let deadline = Instant::now() + timeout;
        let mut status = None;
        loop {
            if status.is_none() {
                status = child.try_wait()?;
            }
            if status.is_some() && stdout_thread.is_finished() && stderr_thread.is_finished() {
                break;
            }
            if Instant::now() >= deadline {
                kill_group(&mut child);
                break;
            }
            thread::sleep(POLL_INTERVAL);
        }

        let Some(status) = status else {
            return Err(format!("timed out after {}s", timeout.as_secs()).into());
        };

        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();

        let first_line = match status.code() {
            Some(code) => format!("exit {code}"),
            None => "killed by signal".to_string(),
        };

        let mut out = first_line;
        let stdout_text = String::from_utf8_lossy(&stdout);
        if !stdout_text.is_empty() {
            out.push('\n');
            out.push_str(&stdout_text);
        }
        let stderr_text = String::from_utf8_lossy(&stderr);
        if !stderr_text.is_empty() {
            out.push_str("\n--- stderr ---\n");
            out.push_str(&stderr_text);
        }

        if out.chars().count() > OUTPUT_LIMIT {
            let truncated: String = out.chars().take(OUTPUT_LIMIT).collect();
            out = format!("{truncated}\n[output truncated at {OUTPUT_LIMIT} characters]");
        }

        Ok(ToolOutput::text(out))
    }
}

/// Reads a pipe to its end on its own thread, so a command that fills
/// one pipe never blocks on the other.
fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        buf
    })
}

/// Kills the command's whole process group - the shell and everything
/// it started - then reaps the shell. `process_group(0)` made the
/// group's id the shell's pid, and `kill` takes a negated pid for a
/// group. Killing closes the pipes, so the readers finish too.
fn kill_group(child: &mut Child) {
    let _ = Command::new("kill")
        .arg("-KILL")
        .arg(format!("-{}", child.id()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests;
