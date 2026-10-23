use std::collections::BTreeMap;
use std::path::Path;

use super::Schema;
use crate::shared::Result;

/// What a map is for, how to write it, and the schema it follows, if it has one.
pub struct Skill {
    name: String,
    description: String,
    instructions: String,
    schema: Option<Schema>,
}

impl Skill {
    pub fn new(name: String, description: String, instructions: String, schema: Option<Schema>) -> Self {
        Self { name, description, instructions, schema }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
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

/// Reads the skill a map is declared by.
pub trait SkillStore {
    /// The skill at `path`.
    fn skill(
        &self,
        path: &Path,
    ) -> impl Future<Output = Result<Skill>> + Send;

    /// The skill in each directory of `library`, named by the directory and sorted by name; a missing `library` holds none.
    fn skills(
        &self,
        library: &Path,
    ) -> impl Future<Output = Result<Vec<(String, Result<Skill>)>>> + Send;
}
