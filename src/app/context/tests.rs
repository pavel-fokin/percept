use super::*;
use crate::app::{Harness, MapShape};
use crate::percept::Message;
use crate::testing::{scope, source};

/// Each message as the one string a comparison cares about.
fn contents(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .map(|message| match message {
            Message::Text { content, .. } => content.clone(),
            Message::ToolCall { arguments, .. } => arguments.clone(),
            Message::ToolResult { content } => content.clone(),
        })
        .collect()
}

fn view(events: &[Event]) -> View<'_> {
    View {
        instructions: Some("the rules"),
        events,
        scope: scope(),
        turn_start: Some(0),
        tools: Vec::new(),
        budget_spent: false,
    }
}

#[test]
fn a_tool_round_only_appends_to_the_request_so_its_prefix_is_reusable() {
    let context = Harness::new(Vec::new(), MapShape::Prompt).context;
    let prompt = Event::message_received(Actor::User, "hello".into(), source("t"), None);
    let called = Event::tool_called(
        "read_map".into(),
        "{}".into(),
        source("t"),
        Some(prompt.id()),
    );
    let resulted = Event::tool_resulted("a map".into(), source("t"), Some(called.id()));

    let first = contents(
        &context
            .build(view(std::slice::from_ref(&prompt)))
            .unwrap()
            .messages,
    );
    let second = contents(
        &context
            .build(view(&[prompt, called, resulted]))
            .unwrap()
            .messages,
    );

    let (prefix, time) = first.split_at(first.len() - 1);
    assert!(time[0].starts_with("The current time is"));
    assert_eq!(&second[..prefix.len()], prefix);
}
