/// A slash command the TUI recognises, with the description shown
/// beside it once suggestions exist.
pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
}

pub const MODELS: &str = "/models";
pub const UNDO: &str = "/undo";

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
];

#[cfg(test)]
mod tests;
