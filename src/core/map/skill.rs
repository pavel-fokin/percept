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
}

/// Reads the skills maps are created from.
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
