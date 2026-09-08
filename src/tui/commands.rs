/// A slash command the TUI recognises, with the description shown
/// beside it once suggestions exist.
pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
}

pub const MODELS: &str = "/models";
pub const UNDO: &str = "/undo";
pub const CONTEXT: &str = "/context";
pub const EFFORT: &str = "/effort";

/// Every slash command the TUI knows, in the order suggestions show
/// them.
pub const COMMANDS: &[Command] = &[
    Command {
        name: MODELS,
        description: "switch the model",
    },
    Command {
        name: UNDO,
        description: "put the working tree back as it was before the last turn",
    },
    Command {
        name: CONTEXT,
        description: "show what the model was shown",
    },
    Command {
        name: EFFORT,
        description: "set the model's reasoning effort for this session",
    },
];

#[cfg(test)]
mod tests;
