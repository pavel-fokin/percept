use super::{Result, Translation, agent_message, session_id, shared};

pub(super) fn translate(raw: serde_json::Value) -> Result<Translation> {
    match raw["hook_event_name"].as_str() {
        // Claude Code runs its own forks (prompt suggestions, recaps, compaction) as subagents with no type.
        Some("SubagentStop") if raw["agent_type"] == "" => Ok(Translation::Skip),
        Some("SubagentStop") => Ok(Translation::Record(session_id(&raw)?, agent_message(raw)?)),
        _ => shared(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::record;
    use super::*;
    use crate::core::{Actor, Payload};
    use serde_json::json;

    #[test]
    fn subagent_stop_is_an_agent_message() {
        let raw = json!({"hook_event_name": "SubagentStop", "agent_type": "general-purpose", "last_assistant_message": "done", "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::Message { session, content: "done".into() });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn client_forks_are_skipped() {
        let raw = json!({"hook_event_name": "SubagentStop", "agent_type": "", "last_assistant_message": "1 да", "session_id": "a"});

        let translation = translate(raw).unwrap();

        assert!(matches!(translation, Translation::Skip));
    }
}
