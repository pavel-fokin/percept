use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::percept::{Tool, ToolOutput, ToolSpec};
use crate::tools::Workspace;

/// How long a call runs when the model gives no `timeout_secs`.
const DEFAULT_TIMEOUT_SECS: u64 = 120;

/// The longest a call may run, regardless of what the model asks for.
const MAX_TIMEOUT_SECS: u64 = 600;

/// Output past this many characters is cut, with a trailer saying so.
const OUTPUT_LIMIT: usize = 30_000;

/// The `bash` tool: runs a shell command at the workspace root and
/// reports its exit code, stdout, and stderr. Not interactive - a
/// command that waits on stdin hangs until it times out.
pub struct Bash {
    workspace: Arc<Workspace>,
}

impl Bash {
    pub fn new(workspace: Arc<Workspace>) -> Self {
        Self { workspace }
    }
}

const NAME: &str = "bash";

const DESCRIPTION: &str = "Run a shell command at the workspace root \
    with `sh -c`. Not interactive - a command that waits on stdin \
    hangs until it times out. Output is truncated past 30000 \
    characters. The first line of the result is `exit {code}` (or \
    `killed by signal` if the process was killed), followed by stdout, \
    and then stderr under a `--- stderr ---` marker when there is any.";

const PARAMETERS: &str = r#"{
  "type": "object",
  "properties": {
    "command": {"type": "string", "description": "the shell command to run"},
    "timeout_secs": {"type": "integer", "minimum": 1, "description": "how long to wait before killing the command; defaults to 120, capped at 600"}
  },
  "required": ["command"],
  "additionalProperties": false
}"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    command: String,
    timeout_secs: Option<u64>,
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
        let timeout = Duration::from_secs(
            args.timeout_secs
                .unwrap_or(DEFAULT_TIMEOUT_SECS)
                .min(MAX_TIMEOUT_SECS),
        );

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&args.command)
            .current_dir(self.workspace.root())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdout_pipe = child.stdout.take().expect("stdout was piped");
        let mut stderr_pipe = child.stderr.take().expect("stderr was piped");

        let stdout_thread = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stdout_pipe.read_to_end(&mut buf);
            buf
        });
        let stderr_thread = thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = stderr_pipe.read_to_end(&mut buf);
            buf
        });

        const POLL_INTERVAL: Duration = Duration::from_millis(50);
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if Instant::now() >= deadline {
                break None;
            }
            thread::sleep(POLL_INTERVAL);
        };

        let Some(status) = status else {
            let _ = child.kill();
            let _ = child.wait();
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

#[cfg(test)]
mod tests;
