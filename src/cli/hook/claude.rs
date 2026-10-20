use super::{EventBuilder, Result, Translation, joined, mcp_output, session_id, shared, str_field, text, unknown_tool_io};
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
        _ => shared(raw, tool_io),
    }
}

fn tool_io(tool: &str, raw: &serde_json::Value) -> (Option<String>, Option<String>) {
    let (input, response) = (&raw["tool_input"], &raw["tool_response"]);

    match tool {
        "Bash" => {
            let streams = ["stdout", "stderr"].iter().filter_map(|stream| response[stream].as_str()).filter(|text| !text.is_empty());
            (str_field(input, "command"), joined(streams.collect()))
        }
        "Read" | "Write" => (str_field(input, "file_path"), None),
        "Edit" => {
            let hunks = response["structuredPatch"].as_array().into_iter().flatten();
            let lines = hunks.flat_map(|hunk| hunk["lines"].as_array().into_iter().flatten()).filter_map(|line| line.as_str());
            (str_field(input, "file_path"), joined(lines.collect()))
        }
        "Grep" | "Glob" => (str_field(input, "pattern"), None),
        "WebFetch" => (str_field(input, "url"), None),
        "WebSearch" => (str_field(input, "query"), None),
        "Skill" => (str_field(input, "skill"), None),
        mcp if mcp.starts_with("mcp__") => (str_field(input, "title"), mcp_output(response)),
        _ => unknown_tool_io(raw),
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

        assert_eq!(event.payload, Payload::ToolUsed { session, tool: "Agent".into(), input: Some(r#"{"description":"Look"}"#.into()), output: Some("{}".into()) });
    }

    fn io(raw: serde_json::Value) -> (Option<String>, Option<String>) {
        let tool = raw["tool_name"].as_str().unwrap().to_owned();
        tool_io(&tool, &raw)
    }

    fn some(text: &str) -> Option<String> {
        Some(text.to_owned())
    }

    #[test]
    fn bash_joins_non_empty_streams() {
        let both = json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_response": {"stdout": "a", "stderr": "b"}});
        let quiet = json!({"tool_name": "Bash", "tool_input": {"command": ""}, "tool_response": {"stdout": "", "stderr": ""}});
        let err_only = json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_response": {"stdout": "", "stderr": "b"}});

        assert_eq!(io(both), (some("ls"), some("a\nb")));
        assert_eq!(io(quiet), (None, None));
        assert_eq!(io(err_only), (some("ls"), some("b")));
    }

    #[test]
    fn file_tools_record_the_path() {
        for tool in ["Read", "Write"] {
            let raw = json!({"tool_name": tool, "tool_input": {"file_path": "/a.rs"}, "tool_response": {"content": "x"}});

            assert_eq!(io(raw), (some("/a.rs"), None));
        }
    }

    #[test]
    fn edit_outputs_the_patch_lines() {
        let patch = json!([{"lines": ["-old", "+new"]}, {"lines": [" ctx"]}]);
        let raw = json!({"tool_name": "Edit", "tool_input": {"file_path": "/a.rs"}, "tool_response": {"structuredPatch": patch}});

        assert_eq!(io(raw), (some("/a.rs"), some("-old\n+new\n ctx")));
    }

    #[test]
    fn lookup_tools_record_their_query() {
        let cases = [("Grep", "pattern"), ("Glob", "pattern"), ("WebFetch", "url"), ("WebSearch", "query"), ("Skill", "skill")];

        for (tool, field) in cases {
            let raw = json!({"tool_name": tool, "tool_input": {field: "q"}, "tool_response": {}});

            assert_eq!(io(raw), (some("q"), None));
        }
    }

    #[test]
    fn a_known_tool_missing_its_field_has_none() {
        let raw = json!({"tool_name": "Read", "tool_input": {"path": "/a.rs"}});

        assert_eq!(io(raw), (None, None));
    }

    #[test]
    fn mcp_tools_output_their_text_parts() {
        let content = json!([{"type": "text", "text": "one"}, {"type": "image", "data": "x"}, {"type": "text", "text": "two"}]);
        let raw = json!({"tool_name": "mcp__docs__read", "tool_input": {"title": "T"}, "tool_response": {"content": content}});

        assert_eq!(io(raw), (some("T"), some("one\ntwo")));
    }

    #[test]
    fn an_unknown_tool_records_its_input_and_response_as_text() {
        let compact = json!({"tool_name": "SendMessage", "tool_input": {"to": "a"}, "tool_response": {"ok": true}});
        let string = json!({"tool_name": "SendMessage", "tool_input": {}, "tool_response": "sent"});
        let texts = json!({"tool_name": "SendMessage", "tool_input": {}, "tool_response": [{"text": "x"}, {"text": "y"}]});
        let missing = json!({"tool_name": "SendMessage"});

        assert_eq!(io(compact), (some(r#"{"to":"a"}"#), some("{\n  \"ok\": true\n}")));
        assert_eq!(io(string).1, some("sent"));
        assert_eq!(io(texts).1, some("x\ny"));
        assert_eq!(io(missing), (None, None));
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
