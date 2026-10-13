use super::{EventBuilder, Result, Translation, session_id, shared, text};
use crate::core::{Actor, ClientAgentId, Event};

const AGENT_MESSAGE: &str = "<agent-message from=\"";
const HAND_BACK: &str = "[Subagent hand-back]";

pub(super) fn translate(raw: serde_json::Value) -> Result<Translation> {
    match raw["hook_event_name"].as_str() {
        // Claude Code runs its own forks (prompt suggestions, recaps, compaction) as subagents with no type.
        Some("SubagentStop") if raw["agent_type"] == "" => Ok(Translation::Skip),
        Some("SubagentStop") => Ok(Translation::Record(session_id(&raw)?, subagent_stop(raw)?)),
        Some("PostToolUse") if raw["tool_name"] == "Agent" && raw["tool_response"]["agentId"].is_string() => {
            Ok(Translation::Record(session_id(&raw)?, subagent_started(raw)?))
        }
        Some("UserPromptSubmit") if text(&raw, "prompt")?.starts_with("<task-notification>") => {
            Ok(Translation::Record(session_id(&raw)?, task_notice(raw)?))
        }
        Some("UserPromptSubmit") if is_hand_back(&text(&raw, "prompt")?) => {
            Ok(Translation::Record(session_id(&raw)?, hand_back(raw)?))
        }
        _ => shared(raw),
    }
}

fn subagent_started(raw: serde_json::Value) -> Result<EventBuilder> {
    let subagent = ClientAgentId::new(text(&raw["tool_response"], "agentId")?);
    let agent = raw["tool_input"]["subagent_type"].as_str().unwrap_or("general-purpose").to_owned();
    let task = text(&raw["tool_input"], "description")?;

    Ok(Box::new(move |source, s| Event::subagent_started(source, s, subagent, &agent, &task, raw)))
}

fn subagent_stop(raw: serde_json::Value) -> Result<EventBuilder> {
    let subagent = ClientAgentId::new(text(&raw, "agent_id")?);
    let content = text(&raw, "last_assistant_message")?;

    Ok(Box::new(move |source, s| Event::subagent_reported(source, s, subagent, &content, raw)))
}

/// A subagent that ran in the background hands its report back as a prompt from the agent.
fn is_hand_back(prompt: &str) -> bool {
    prompt.starts_with(AGENT_MESSAGE) && prompt.contains(HAND_BACK)
}

fn hand_back(raw: serde_json::Value) -> Result<EventBuilder> {
    let prompt = text(&raw, "prompt")?;
    let subagent = between(&prompt, AGENT_MESSAGE, "\"").ok_or("unterminated agent-message sender")?;
    let subagent = ClientAgentId::new(subagent.to_owned());
    let content = report(&prompt);

    Ok(Box::new(move |source, s| Event::subagent_reported(source, s, subagent, &content, raw)))
}

/// The lines after the one that introduces the report, without the two-space indent the client adds to each.
fn report(prompt: &str) -> String {
    const INTRO: &str = "The report follows:";

    let body = &prompt[..prompt.find("</agent-message>").unwrap_or(prompt.len())];
    let body = body.find(INTRO).map_or(body, |at| &body[at + INTRO.len()..]);
    let lines = body.split_once('\n').map_or("", |(_, lines)| lines);

    let dedented: Vec<&str> = lines.lines().map(|line| line.strip_prefix("  ").unwrap_or(line)).collect();
    dedented.join("\n").trim_end().to_owned()
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
    fn launching_a_subagent_records_its_start() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Agent", "tool_input": {"description": "Build map", "prompt": "p", "subagent_type": "software-developer"}, "tool_response": {"agentId": "a84", "isAsync": true}, "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(
            event.payload,
            Payload::SubagentStarted { session, subagent: ClientAgentId::new("a84".into()), agent: "software-developer".into(), task: "Build map".into() }
        );
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn launching_without_a_type_is_a_general_purpose_subagent() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Agent", "tool_input": {"description": "Look"}, "tool_response": {"agentId": "a84"}, "session_id": "a"});

        let (_, session, event) = record(translate(raw).unwrap());

        assert_eq!(
            event.payload,
            Payload::SubagentStarted { session, subagent: ClientAgentId::new("a84".into()), agent: "general-purpose".into(), task: "Look".into() }
        );
    }

    #[test]
    fn launching_without_an_agent_id_is_a_tool_use() {
        let raw = json!({"hook_event_name": "PostToolUse", "tool_name": "Agent", "tool_input": {"description": "Look"}, "tool_response": {}, "session_id": "a"});

        let (_, session, event) = record(translate(raw).unwrap());

        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Agent".into() });
    }

    #[test]
    fn a_hand_back_is_a_subagent_report_without_the_indent() {
        let prompt = "<agent-message from=\"a84\">\n[Subagent hand-back] The text below is the final report. The report follows:\n  line one\n  \n  **Changed**\n  - item\n</agent-message>";
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt, "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(
            event.payload,
            Payload::SubagentReported { session, subagent: ClientAgentId::new("a84".into()), content: "line one\n\n**Changed**\n- item".into() }
        );
        assert_eq!(event.raw, Some(raw));
    }

    #[test]
    fn a_hand_back_without_the_report_line_keeps_the_whole_body() {
        let prompt = "<agent-message from=\"a84\">\n[Subagent hand-back] Here it is\n  text\n</agent-message>";
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt, "session_id": "a"});

        let (_, session, event) = record(translate(raw).unwrap());

        assert_eq!(
            event.payload,
            Payload::SubagentReported { session, subagent: ClientAgentId::new("a84".into()), content: "[Subagent hand-back] Here it is\ntext".into() }
        );
    }

    #[test]
    fn an_agent_message_without_the_marker_is_a_human_message() {
        let prompt = "<agent-message from=\"other\">\nhello\n</agent-message>";
        let raw = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt, "session_id": "a"});

        let (_, session, event) = record(translate(raw).unwrap());

        assert_eq!(event.actor, Actor::Human);
        assert_eq!(event.payload, Payload::Message { session, content: prompt.into() });
    }

    #[test]
    fn a_typed_subagent_stop_is_a_subagent_report() {
        let raw = json!({"hook_event_name": "SubagentStop", "agent_id": "a1", "agent_type": "general-purpose", "last_assistant_message": "done", "session_id": "a"});

        let (_, session, event) = record(translate(raw.clone()).unwrap());

        assert_eq!(event.actor, Actor::Agent);
        assert_eq!(event.payload, Payload::SubagentReported { session, subagent: ClientAgentId::new("a1".into()), content: "done".into() });
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
