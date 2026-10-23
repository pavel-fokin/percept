use std::collections::BTreeMap;
use std::path::Path;

use super::Schema;
use crate::shared::Result;

/// What a map is for, how to write it, and the schema it follows, if it has one.
pub struct Definition {
    purpose: String,
    instructions: String,
    schema: Option<Schema>,
}

impl Definition {
    pub fn new(purpose: String, instructions: String, schema: Option<Schema>) -> Self {
        Self { purpose, instructions, schema }
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// Without a schema, any node kind and property is allowed.
    pub fn check_node(&self, kind: &str, properties: &BTreeMap<String, String>) -> Result<()> {
        self.schema.as_ref().map_or(Ok(()), |s| s.check_node(kind, properties))
    }

    /// Without a schema, any edge kind between any node kinds is allowed.
    pub fn check_edge(&self, kind: &str, from: &str, to: &str) -> Result<()> {
        self.schema.as_ref().map_or(Ok(()), |s| s.check_edge(kind, from, to))
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
