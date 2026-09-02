/// A capability the model invokes by name during a turn. The domain
/// owns the shape; the implementation lives in `store`, the way
/// `EventLog` is a domain port that `store` implements.
pub trait Tool: Send + Sync {
    /// The name, description, and parameter schema a provider hands the
    /// model so it can call the tool.
    fn spec(&self) -> ToolSpec;

    /// Runs the tool against `arguments` - JSON text the model
    /// produced, matching `spec().parameters`. `Ok` is the text fed
    /// back as the result; an `Err`'s message becomes that text
    /// instead, so a bad call still gives the model something to read.
    fn run(&self, arguments: &str) -> Result<String, Box<dyn std::error::Error>>;
}

/// What the model is told about a `Tool`. `parameters` is a JSON Schema
/// as text: the domain stays serde-free, so a provider parses it when
/// building its request.
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
}
