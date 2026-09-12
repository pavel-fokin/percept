use serde_json::json;

use super::*;
use crate::core::testing::Fixture;

fn as_map(existing: Value) -> JsonMap<String, Value> {
    existing.as_object().unwrap().clone()
}

fn claude_code(existing: Value, command: &str) -> Value {
    merge(as_map(existing), command, &EVENTS, &CLAUDE_ALLOW).unwrap()
}

fn codex(existing: Value, command: &str) -> Value {
    merge(as_map(existing), command, &EVENTS, &[]).unwrap()
}

fn init(client: &str, capture: bool) -> InitArgs {
    InitArgs {
        client: client.to_string(),
        capture,
    }
}

#[test]
fn writes_claude_code_from_nothing() {
    let root = claude_code(json!({}), "percept hook claude-code");

    assert_eq!(
        root["hooks"]["UserPromptSubmit"],
        json!([{ "hooks": [{ "type": "command", "command": "percept hook claude-code", "timeout": 20 }] }])
    );
    assert_eq!(
        root["hooks"]["PostToolUse"][0]["hooks"][0]["command"],
        "percept hook claude-code"
    );
    assert_eq!(
        root["hooks"]["Stop"][0]["hooks"][0]["command"],
        "percept hook claude-code"
    );
    assert_eq!(
        root["permissions"]["allow"],
        json!([
            "Bash(percept maps *)",
            "Bash(percept events *)",
            "Bash(percept start*)"
        ])
    );
}

#[test]
fn writes_codex_from_nothing() {
    let root = codex(json!({}), "percept hook codex");

    for event in EVENTS {
        assert_eq!(
            root["hooks"][event][0]["hooks"][0]["command"],
            "percept hook codex"
        );
    }
    assert!(root.get("permissions").is_none());
}

#[test]
fn keeps_unrelated_keys() {
    let existing = json!({ "model": "opus", "other": { "nested": true } });

    let root = claude_code(existing, "percept hook claude-code");

    assert_eq!(root["model"], "opus");
    assert_eq!(root["other"], json!({ "nested": true }));
}

#[test]
fn does_not_duplicate_an_existing_hook_entry() {
    let existing = claude_code(json!({}), "percept hook claude-code");

    let merged = claude_code(existing.clone(), "percept hook claude-code");

    assert_eq!(merged, existing);
    assert_eq!(
        merged["hooks"]["UserPromptSubmit"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn does_not_duplicate_an_existing_allow_entry() {
    let existing = claude_code(json!({}), "percept hook claude-code");

    let merged = claude_code(existing.clone(), "percept hook claude-code");

    assert_eq!(merged["permissions"]["allow"].as_array().unwrap().len(), 3);
}

#[test]
fn keeps_another_command_under_the_same_event() {
    let existing = json!({
        "hooks": {
            "UserPromptSubmit": [
                { "hooks": [{ "type": "command", "command": "echo other", "timeout": 5 }] }
            ],
            "PostToolUse": [],
            "Stop": []
        }
    });

    let root = claude_code(existing, "percept hook claude-code");

    let entries = root["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["hooks"][0]["command"], "echo other");
    assert_eq!(
        entries[1]["hooks"][0]["command"],
        "percept hook claude-code"
    );
}

#[test]
fn a_matcher_scoped_entry_does_not_count_as_the_unscoped_one() {
    let existing = json!({
        "hooks": {
            "PostToolUse": [
                {
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "percept hook claude-code" }]
                }
            ],
            "UserPromptSubmit": [],
            "Stop": []
        }
    });

    let root = claude_code(existing, "percept hook claude-code");

    let entries = root["hooks"]["PostToolUse"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["matcher"], "Bash");
    assert_eq!(entries[1]["hooks"][0]["command"], "percept hook claude-code");
    assert!(entries[1].get("matcher").is_none());
}

#[test]
fn second_run_is_identical() {
    let once = codex(json!({}), "percept hook codex");
    let twice = codex(once.clone(), "percept hook codex");

    assert_eq!(once, twice);
}

#[test]
fn unknown_client_is_refused() {
    let temp = tempfile::tempdir().unwrap();

    let err = run(
        init("cursor", true),
        temp.path(),
    )
    .unwrap_err();

    assert!(err.to_string().contains("claude-code"));
    assert!(err.to_string().contains("codex"));
}

#[test]
fn a_non_object_file_is_an_error() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(temp.path().join(".claude/settings.json"), "[1, 2]").unwrap();

    let err = run(
        init("claude-code", true),
        temp.path(),
    )
    .unwrap_err();

    assert!(err.to_string().contains("not a JSON object"));
    let text = std::fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap();
    assert_eq!(text, "[1, 2]");
}

#[test]
fn an_empty_file_is_treated_as_no_config() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(temp.path().join(".claude/settings.json"), "  \n").unwrap();

    run(
        init("claude-code", true),
        temp.path(),
    )
    .unwrap();

    let text = std::fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
        "percept hook claude-code"
    );
}

#[test]
fn writes_events_and_entry_fields_in_declared_order() {
    let temp = tempfile::tempdir().unwrap();

    run(
        init("codex", true),
        temp.path(),
    )
    .unwrap();

    let text = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    let submit = text.find("UserPromptSubmit").unwrap();
    let post = text.find("PostToolUse").unwrap();
    let stop = text.find("\"Stop\"").unwrap();
    assert!(submit < post && post < stop, "events must read {}", EVENTS.join(", "));

    let kind = text.find("\"type\"").unwrap();
    let command = text.find("\"command\"").unwrap();
    let timeout = text.find("\"timeout\"").unwrap();
    assert!(kind < command && command < timeout, "an entry must read type, command, timeout");
}

#[test]
fn without_capture_init_writes_no_tool_use_hook() {
    let temp = tempfile::tempdir().unwrap();

    run(init("codex", false), temp.path()).unwrap();

    let text = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let hooks = value["hooks"].as_object().unwrap();
    let written: Vec<&str> = hooks.keys().map(String::as_str).collect();
    assert_eq!(written, ["SessionStart", "UserPromptSubmit", "Stop"]);
}

#[test]
fn a_later_init_without_capture_keeps_the_tool_use_hook() {
    let temp = tempfile::tempdir().unwrap();

    run(init("codex", true), temp.path()).unwrap();
    let with = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();
    run(init("codex", false), temp.path()).unwrap();
    let after = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();

    assert_eq!(with, after);
}

#[test]
fn writes_claude_code_settings_to_disk() {
    let temp = tempfile::tempdir().unwrap();

    run(
        init("claude-code", true),
        temp.path(),
    )
    .unwrap();

    let text = std::fs::read_to_string(temp.path().join(".claude/settings.json")).unwrap();
    assert!(text.ends_with('\n'));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
        "percept hook claude-code"
    );
}

#[test]
fn init_writes_the_shipped_schemas() {
    let fixture = Fixture::new();

    run(init("claude-code", true), fixture.path()).unwrap();

    let schemas = mapstore::load_schemas(fixture.path()).unwrap();
    let names: Vec<&str> = schemas.folded().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["concepts", "decisions"]);
}

#[test]
fn init_keeps_an_existing_schema_file() {
    let fixture = Fixture::new();
    let other = "name = \"decisions\"\npurpose = \"p\"\n\n[[node]]\nkind = \"decision\"\n";
    fixture.write(".percept/schemas/decisions.toml", other);

    run(init("claude-code", true), fixture.path()).unwrap();

    let text = std::fs::read_to_string(fixture.path().join(".percept/schemas/decisions.toml")).unwrap();
    assert_eq!(text, other);
}

#[test]
fn running_init_twice_leaves_the_file_unchanged() {
    let temp = tempfile::tempdir().unwrap();

    run(
        init("codex", true),
        temp.path(),
    )
    .unwrap();
    let first = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();

    run(
        init("codex", true),
        temp.path(),
    )
    .unwrap();
    let second = std::fs::read_to_string(temp.path().join(".codex/hooks.json")).unwrap();

    assert_eq!(first, second);
}
