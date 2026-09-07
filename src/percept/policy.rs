/// Whether a tool call the model asked for may run. Checked once per
/// call, after `tool.called` is committed and before anything runs, so
/// the log always shows what the model asked for, and the verdict
/// decides only what happened next.
pub trait Policy: Send + Sync {
    /// `arguments` is the JSON text the model produced, so a policy
    /// can look inside a call - the command a `bash` call would run -
    /// and not only at the tool's name.
    fn check(&self, tool: &str, arguments: &str) -> Verdict;
}

/// What a `Policy` says about one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Run it.
    Allow,
    /// Put it to the user; run it if they say so.
    Ask,
    /// Do not run it. The text goes back to the model as the call's
    /// result, so it can do something else. No policy denies yet; a
    /// denylist would.
    #[allow(dead_code)]
    Deny(String),
}

/// The policy with no rules: every call runs. What a caller with no
/// user to ask, or one who has said yes to everything, uses.
pub struct AllowAll;

impl Policy for AllowAll {
    fn check(&self, _tool: &str, _arguments: &str) -> Verdict {
        Verdict::Allow
    }
}
