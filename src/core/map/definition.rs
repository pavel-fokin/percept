use std::path::Path;

use super::Schema;
use crate::shared::Result;

/// What a map is for, how to write it, and the schema it follows.
pub struct Definition {
    purpose: String,
    instructions: String,
    schema: Schema,
}

impl Definition {
    pub fn new(purpose: String, instructions: String, schema: Schema) -> Self {
        Self { purpose, instructions, schema }
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
}

/// Reads the definition a map is declared by.
pub trait DefinitionStore {
    /// The definition in `file`.
    fn definition(
        &self,
        file: &Path,
    ) -> impl Future<Output = Result<Definition>> + Send;
}
