use super::{Result, Translation, shared};

pub(super) fn translate(raw: serde_json::Value) -> Result<Translation> {
    shared(raw)
}

#[cfg(test)]
mod tests {
    use super::super::tests::record;
    use super::*;
    use crate::core::{Actor, Payload};
    use serde_json::json;

    #[test]
    fn subagent_stop_is_an_error() {
        let raw = json!({"hook_event_name": "SubagentStop", "last_assistant_message": "done", "session_id": "a"});

        assert!(translate(raw).is_err());
    }

    #[test]
    fn shared_hooks_are_recorded() {
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": "hi", "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: "hi".into() });
        assert_eq!(event.raw, Some(raw));
    }
}
