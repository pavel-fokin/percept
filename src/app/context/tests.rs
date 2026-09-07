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

/// No turn streaming and no window reported, so history is cut by the
/// window's floor alone.
fn view(events: &[Event]) -> View<'_> {
    View {
        instructions: Some("the rules"),
        events,
        scope: scope(),
        turn_start: None,
        context_window: None,
        tools: Vec::new(),
        budget_spent: false,
    }
}

/// A prompt of `tokens` tokens, named so a test can find it.
fn prompt(actor: Actor, name: &str, tokens: usize) -> Event {
    let content = format!("{name:<width$}", width = tokens * 4);
    Event::message_received(actor, content, source("t"), None)
}

/// Fills to 100 tokens, cuts back to 50.
fn history_only() -> Context {
    Context {
        sections: vec![Section::History(Window {
            share: 0.5,
            keep: 0.25,
            floor: 100,
        })],
    }
}

fn first_word(message: &str) -> &str {
    message.split_whitespace().next().unwrap()
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

#[test]
fn history_holds_still_until_it_fills_then_cuts_back_to_the_keep_mark() {
    let context = history_only();
    let mut events: Vec<Event> = (0..10)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();

    // Ten prompts of ten tokens fill the window exactly: all in.
    let sent = contents(&context.build(view(&events)).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "p0");

    // The eleventh overflows it: cut back to fifty tokens, five prompts.
    events.push(prompt(Actor::User, "p10", 10));
    let sent = contents(&context.build(view(&events)).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "p6");
    assert_eq!(sent.len(), 5);

    // The twelfth fits under the high mark: the start does not move.
    events.push(prompt(Actor::User, "p11", 10));
    let sent = contents(&context.build(view(&events)).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "p6");
    assert_eq!(sent.len(), 6);
}

#[test]
fn a_cut_lands_on_the_next_user_prompt_never_on_a_reply() {
    let context = history_only();
    // Five user prompts of ten tokens with four replies of fifteen
    // between them: 110 tokens. Cutting back to fifty stops on the
    // third reply, so the window opens on the prompt after it.
    let mut events = Vec::new();
    for i in 0..5 {
        events.push(prompt(Actor::User, &format!("u{i}"), 10));
        if i < 4 {
            events.push(prompt(Actor::Model, &format!("m{i}"), 15));
        }
    }

    let sent = contents(&context.build(view(&events)).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "u3");
    assert_eq!(sent.len(), 3);
}

#[test]
fn a_reported_context_window_scales_the_marks() {
    let context = history_only();
    let events: Vec<Event> = (0..10)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();

    // A 100-token window halves the marks to fifty and twenty-five:
    // ten prompts overflow twice, and the last cut keeps two.
    let view = View {
        context_window: Some(100),
        ..view(&events)
    };
    let sent = contents(&context.build(view).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "p8");
}
