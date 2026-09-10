//! The one path a claim's standing changes through: `confirm` and
//! `dispute` fold the map, resolve the node named, and append the
//! event that records the judgment - so the rule that a user-written
//! node carries no standing to judge, and that a dispute always says
//! why, lives once. `maps confirm`, `maps dispute`, and a review
//! page's writes all call these.

use std::error::Error;
use std::fmt;

use crate::core::{Actor, Event, EventLog, HumanId, Map, MapError, NodeId, Schemas, Source};

use super::fold_map;

/// What can go wrong confirming or disputing a claim.
#[derive(Debug)]
pub(crate) enum JudgeError {
    /// `s` names no node the map holds, or names one ambiguously.
    Map(MapError),
    /// `s` is the human's own landmark, not a model's claim.
    UsersOwn(String),
    /// A dispute with nothing said for why.
    BlankWhy,
    /// Folding the map or appending the event failed.
    Log(Box<dyn Error>),
}

impl fmt::Display for JudgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Map(err) => write!(f, "{err}"),
            Self::UsersOwn(s) => {
                write!(f, "{s} is the user's own; standing is for a model's claim")
            }
            Self::BlankWhy => write!(f, "a dispute must say why"),
            Self::Log(err) => write!(f, "{err}"),
        }
    }
}

impl Error for JudgeError {}

/// Resolves `s` against `map`, refusing a node the map does not hold or
/// one the human wrote themselves - a landmark, not a model's claim, so
/// it carries no standing to judge.
pub(crate) fn resolve_judged_node(map: &Map, s: &str) -> Result<NodeId, JudgeError> {
    let id = map.resolve_str(s).map_err(JudgeError::Map)?;
    let node = map.node(id).expect("resolve_str returns a live node's id");
    if matches!(node.actor, Actor::Human(_)) {
        return Err(JudgeError::UsersOwn(s.to_string()));
    }
    Ok(id)
}

/// Marks `node`'s claim confirmed on `map_name` - always the human's
/// own judgment, never the model's. `me` is who judged it, from
/// `Jsonl::me`.
pub(crate) fn confirm(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map_name: &str,
    node: &str,
) -> Result<Event, JudgeError> {
    let map =
        fold_map(log, schemas, map_name, &source.scope()).map_err(JudgeError::Log)?;
    let node = resolve_judged_node(&map, node)?;
    let event = Event::claim_confirmed(map_name.to_string(), node, me, source.clone(), None);
    log.append(&event).map_err(JudgeError::Log)?;
    Ok(event)
}

/// Marks `node`'s claim disputed on `map_name`, with `why` - always the
/// human's own judgment. Refuses a blank `why`, here, so no writer can
/// append a dispute without saying why.
pub(crate) fn dispute(
    log: &dyn EventLog,
    schemas: &Schemas,
    source: &Source,
    me: Option<HumanId>,
    map_name: &str,
    node: &str,
    why: String,
) -> Result<Event, JudgeError> {
    if why.trim().is_empty() {
        return Err(JudgeError::BlankWhy);
    }
    let map =
        fold_map(log, schemas, map_name, &source.scope()).map_err(JudgeError::Log)?;
    let node = resolve_judged_node(&map, node)?;
    let event = Event::claim_disputed(map_name.to_string(), node, why, me, source.clone(), None);
    log.append(&event).map_err(JudgeError::Log)?;
    Ok(event)
}

#[cfg(test)]
mod tests;
