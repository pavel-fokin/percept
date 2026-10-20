use super::{Result, Translation, json_input, mcp_output, non_empty, shared, str_field, unknown_tool_io};

const PATCH_FILE_LINES: [&str; 4] = ["*** Add File: ", "*** Update File: ", "*** Delete File: ", "*** Move to: "];

pub(super) fn translate(raw: serde_json::Value) -> Result<Translation> {
    shared(raw, tool_io)
}

fn tool_io(tool: &str, raw: &serde_json::Value) -> (Option<String>, Option<String>) {
    let (input, response) = (&raw["tool_input"], &raw["tool_response"]);

    match tool {
        "Bash" => (str_field(input, "command"), non_empty(response.as_str())),
        "apply_patch" => {
            let patch = input["command"].as_str().unwrap_or_default();
            let paths: Vec<&str> = patch
                .lines()
                .filter_map(|line| PATCH_FILE_LINES.iter().find_map(|prefix| line.strip_prefix(prefix)))
                .collect();
            (non_empty(Some(&paths.join(", "))), non_empty(Some(patch)))
        }
        "view_image" => (str_field(input, "path"), None),
        mcp if mcp.starts_with("mcp__") => (json_input(raw), mcp_output(response)),
        _ => unknown_tool_io(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{record, some};
    use super::*;
    use crate::core::{Actor, Payload};
    use serde_json::json;

    fn io(raw: serde_json::Value) -> (Option<String>, Option<String>) {
        super::super::tests::io(tool_io, raw)
    }

    #[test]
    fn subagent_stop_is_an_error() {
        let raw = json!({"hook_event_name": "SubagentStop", "last_assistant_message": "done", "session_id": "a"});

        assert!(translate(raw).is_err());
    }

    #[test]
    fn bash_outputs_the_response_string() {
        let raw = json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_response": "a\nb"});

        assert_eq!(io(raw), (some("ls"), some("a\nb")));
    }

    #[test]
    fn apply_patch_lists_the_files_and_outputs_the_patch() {
        let patch = "*** Begin Patch\n*** Add File: a.rs\n+x\n*** Update File: b.rs\n*** Move to: d.rs\n@@\n-y\n*** Delete File: c.rs\n*** End Patch";
        let raw = json!({"tool_name": "apply_patch", "tool_input": {"command": patch}, "tool_response": "ok"});

        assert_eq!(io(raw), (some("a.rs, b.rs, d.rs, c.rs"), some(patch)));
    }

    #[test]
    fn view_image_records_the_path() {
        let raw = json!({"tool_name": "view_image", "tool_input": {"path": "/a.png"}, "tool_response": []});

        assert_eq!(io(raw), (some("/a.png"), None));
    }

    #[test]
    fn mcp_tools_record_their_arguments_and_text_parts() {
        let raw = json!({"tool_name": "mcp__docs__read", "tool_input": {"id": 7}, "tool_response": {"content": [{"type": "text", "text": "one"}]}});

        assert_eq!(io(raw), (some(r#"{"id":7}"#), some("one")));
    }

    #[test]
    fn an_unknown_tool_records_its_input_and_response_as_text() {
        let raw = json!({"tool_name": "webrun", "tool_input": {"url": "u"}, "tool_response": [{"text": "x"}, {"text": "y"}]});

        assert_eq!(io(raw), (some(r#"{"url":"u"}"#), some("x\ny")));
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
