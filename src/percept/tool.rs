use super::Payload;

/// A capability the model invokes by name during a turn. The domain
/// owns the shape; the implementation lives in `store`, the way
/// `EventLog` is a domain port that `store` implements.
pub trait Tool: Send + Sync {
    /// The name, description, and parameter schema a provider hands the
    /// model so it can call the tool.
    fn spec(&self) -> ToolSpec;

    /// Runs the tool against `arguments` - JSON text the model
    /// produced, matching `spec().parameters`. `Ok` carries the text
    /// fed back as the result and any events the call produced, for
    /// `App` to commit; an `Err`'s message becomes that text instead,
    /// so a bad call still gives the model something to read, and
    /// commits nothing.
    fn run(&self, arguments: &str) -> Result<ToolOutput, Box<dyn std::error::Error>>;
}

/// What a tool hands back: the text fed to the model as the result,
/// and any events the call produced, which `App` commits caused by the
/// call. Most tools commit nothing.
pub struct ToolOutput {
    pub content: String,
    pub commits: Vec<Payload>,
}

impl ToolOutput {
    /// The common case: text with nothing to commit.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            commits: Vec::new(),
        }
    }
}

/// What the model is told about a `Tool`. `parameters` is a JSON Schema
/// as text: the domain stays serde-free, so a provider parses it when
/// building its request.
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
}
