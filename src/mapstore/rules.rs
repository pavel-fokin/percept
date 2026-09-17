//! The rules a schema sends into a client's turn, rendered as the text
//! a prompt carries. Beside `start`: one more external form of a
//! project's schemas, read live and never written to a file.

use std::path::Path;

use super::SCHEMAS_DIR;
use crate::core::Schemas;

/// Every loaded schema's own lines for `moment`, in schema order, one
/// per line, under a line naming the directory they came from - no
/// rule text and no gate held here, so a schema that declares none
/// costs the moment nothing. `None` when there are none, since a block
/// that named a source for lines it did not send would teach a reader
/// that percept's own attributions can be false. The directory is an
/// absolute path: a client's working directory may be any
/// subdirectory of `checkout`, and a rule line is evidence only if the
/// reader can open what it names.
pub fn for_moment(schemas: &Schemas, checkout: &Path, moment: &str) -> Option<String> {
    let lines: Vec<&str> = schemas
        .folded()
        .flat_map(|schema| schema.rules.at(moment).iter())
        .map(String::as_str)
        .collect();

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "rules from {}\n{}",
        checkout.join(SCHEMAS_DIR).display(),
        lines.join("\n")
    ))
}
