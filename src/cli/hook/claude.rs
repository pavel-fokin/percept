use super::{EventBuilder, Result, Translation, agent_message, session_id, shared, text};
use crate::core::{Actor, Event};

pub(super) fn translate(raw: serde_json::Value) -> Result<Translation> {
    match raw["hook_event_name"].as_str() {
        // Claude Code runs its own forks (prompt suggestions, recaps, compaction) as subagents with no type.
        Some("SubagentStop") if raw["agent_type"] == "" => Ok(Translation::Skip),
        Some("SubagentStop") => Ok(Translation::Record(session_id(&raw)?, agent_message(raw)?)),
        Some("UserPromptSubmit") if text(&raw, "prompt")?.starts_with("<task-notification>") => {
            Ok(Translation::Record(session_id(&raw)?, task_notice(raw)?))
        }
        _ => shared(raw),
    }
}

/// Claude Code reports a finished background task as a prompt submitted in the agent's name.
fn task_notice(raw: serde_json::Value) -> Result<EventBuilder> {
    let prompt = text(&raw, "prompt")?;
    let content = between(&prompt, "<summary>", "</summary>").unwrap_or(&prompt).to_owned();

    Ok(Box::new(move |source, s| Event::message(source, s, Actor::System, &content, raw)))
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = start + text[start..].find(close)?;
    Some(&text[start..end])
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

    #[test]
    fn task_notification_is_a_system_notice() {
        let prompt = "<task-notification>\n<status>failed</status>\n<summary>Background command \"just dev\" failed</summary>\n</task-notification>";
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt, "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::System);
        assert_eq!(event.payload, Payload::Message { session, content: "Background command \"just dev\" failed".into() });
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn task_notification_without_summary_keeps_the_prompt() {
        let prompt = "<task-notification>\n<status>failed</status>\n</task-notification>";
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt, "session_id": "a"});

        let (_, session, event) = record(translate(raw).unwrap());

        assert_eq!(event.payload, Payload::Message { session, content: prompt.into() });
    }
}
