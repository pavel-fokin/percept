use super::*;
use crate::app::{Harness, MapShape};
use crate::core::testing::{schemas, scope, source};
use crate::harness::Message;

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

/// A `Schemas` living for the whole test binary, so `View`'s borrow has
/// something to point at without every test owning one.
fn test_schemas() -> &'static Schemas {
    static SCHEMAS: std::sync::OnceLock<Schemas> = std::sync::OnceLock::new();
    SCHEMAS.get_or_init(schemas)
}

/// No turn streaming and no window reported, so history is cut by the
/// window's floor alone.
fn view(events: &[Event]) -> View<'_> {
    View {
        instructions: Some("the rules"),
        events,
        schemas: test_schemas(),
        scope: scope(),
        turn_start: None,
        context_window: None,
        reasoning_effort: None,
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
const WINDOW: Window = Window {
    share: 0.5,
    keep: 0.25,
    floor: 100,
};

/// History, then the turn: the two sections the window tests watch.
fn history_then_turn(window: Window, index: usize) -> Context {
    Context {
        sections: vec![Section::History { window, index }, Section::Turn],
    }
}

fn history_only() -> Context {
    history_then_turn(WINDOW, 0)
}

fn first_word(message: &str) -> &str {
    message.split_whitespace().next().unwrap()
}

#[test]
fn build_carries_the_view_s_selected_reasoning_effort_onto_the_request() {
    let context = history_only();
    let events = [prompt(Actor::User, "hi", 1)];
    let view = View {
        reasoning_effort: Some(crate::harness::ReasoningEffort::High),
        ..view(&events)
    };

    let request = context.build(view).unwrap();

    assert_eq!(
        request.reasoning_effort,
        Some(crate::harness::ReasoningEffort::High)
    );
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
    fn streaming(events: &[Event]) -> View<'_> {
        View {
            turn_start: Some(0),
            ..view(events)
        }
    }

    let first = contents(
        &context
            .build(streaming(std::slice::from_ref(&prompt)))
            .unwrap()
            .messages,
    );
    let second = contents(
        &context
            .build(streaming(&[prompt, called, resulted]))
            .unwrap()
            .messages,
    );

    assert_eq!(&second[..first.len()], &first[..]);
    assert_eq!(second.len(), first.len() + 2);
}

#[test]
fn the_time_is_the_turn_s_prompt_time_and_sits_before_the_turn() {
    let context = Harness::new(Vec::new(), MapShape::Prompt).context;
    let events = vec![prompt(Actor::User, "hello", 2)];
    let view = View {
        turn_start: Some(0),
        ..view(&events)
    };

    let sent = contents(&context.build(view).unwrap().messages);

    let time = &sent[sent.len() - 2];
    assert_eq!(
        time,
        &format!("The current time is {}.", events[0].created_at())
    );
    assert_eq!(first_word(&sent[sent.len() - 1]), "hello");
}

#[test]
fn describe_prints_one_line_per_section_with_its_message_count_and_tokens() {
    let context = Harness::new(Vec::new(), MapShape::Prompt).context;
    let events = vec![prompt(Actor::User, "hi", 1)];
    let view = View {
        context_window: Some(1000),
        ..view(&events)
    };

    let report = context.describe(&view).unwrap();
    let lines: Vec<&str> = report.lines().collect();

    assert_eq!(lines.len(), context.sections.len());
    assert!(lines[0].starts_with("instructions"));
    assert!(lines[0].contains("1 message"));
    assert!(lines[0].contains("tokens"));
    assert!(lines[2].contains("(fills to 125, cuts back to 62; index 200)"));
}

#[test]
fn an_old_tool_result_is_cut_to_its_head_while_the_turn_s_own_stays_whole() {
    let context = history_then_turn(
        Window {
            floor: 1000,
            ..WINDOW
        },
        0,
    );
    let long = "x".repeat(500);
    let events = vec![
        prompt(Actor::User, "u0", 2),
        Event::tool_called("read_file".into(), "{}".into(), source("t"), None),
        Event::tool_resulted(long.clone(), source("t"), None),
        prompt(Actor::User, "u1", 2),
        Event::tool_called("read_file".into(), "{}".into(), source("t"), None),
        Event::tool_resulted(long.clone(), source("t"), None),
    ];
    let old_result = events[2].id();

    let view = View {
        turn_start: Some(3),
        ..view(&events)
    };
    let sent = contents(&context.build(view).unwrap().messages);

    assert_eq!(sent.len(), 6);
    assert!(sent[2].starts_with(&"x".repeat(PREVIEW_CHARS)));
    assert!(sent[2].ends_with(&format!(
        "[500 chars; read_event {} opens the whole]",
        old_result.as_uuid()
    )));
    assert_eq!(sent[5], long);
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
fn a_result_larger_than_the_low_mark_leaves_the_window_opening_on_the_next_prompt() {
    let context = history_only();
    // A cut result is a head plus a handle, under fifty tokens; a
    // reply of sixty overflows on its own and evicts everything before
    // it, itself included, so history opens on the prompt after it.
    let events = vec![
        prompt(Actor::User, "u0", 10),
        Event::tool_called("bash".into(), "{}".into(), source("t"), None),
        Event::tool_resulted("y".repeat(2000), source("t"), None),
        prompt(Actor::Model, "m0", 60),
        prompt(Actor::User, "u1", 10),
        prompt(Actor::Model, "m1", 10),
    ];

    let sent = contents(&context.build(view(&events)).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "u1");
    assert_eq!(sent.len(), 2);
}

#[test]
fn the_turn_s_own_events_never_move_where_history_starts() {
    let context = history_only();
    // Nine prompts of ten tokens sit under the high mark; the turn
    // then reads a file five times over. History still opens on p0.
    let mut events: Vec<Event> = (0..9)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();
    events.push(prompt(Actor::User, "the question", 3));
    for _ in 0..5 {
        events.push(Event::tool_called(
            "read_file".into(),
            "{}".into(),
            source("t"),
            None,
        ));
        events.push(Event::tool_resulted("z".repeat(400), source("t"), None));
    }
    let view = View {
        turn_start: Some(9),
        ..view(&events)
    };

    let sent = contents(&context.build(view).unwrap().messages);
    assert_eq!(first_word(&sent[0]), "p0");
    assert_eq!(first_word(&sent[9]), "the");
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

#[test]
fn events_just_past_the_window_are_indexed_one_line_each_with_their_ids() {
    let context = history_then_turn(WINDOW, 2);
    // Eleven prompts of ten tokens: the window opens on p6, and the
    // two before it are indexed.
    let events: Vec<Event> = (0..11)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();

    let sent = contents(&context.build(view(&events)).unwrap().messages);

    let index = &sent[0];
    assert!(index.starts_with("Before the messages below"));
    assert!(!index.contains("p3"));
    assert!(index.contains(&format!("{} user: p4", events[4].id().as_uuid())));
    assert!(index.contains("user: p5"));
    assert_eq!(first_word(&sent[1]), "p6");
}

#[test]
fn the_index_takes_at_most_what_history_keeps_after_a_cut() {
    let context = history_then_turn(WINDOW, 200);
    // Twenty-one prompts of ten tokens: the last cut opens history on
    // p12. An index line is about twenty-one tokens, so the low mark
    // of fifty admits two lines, the newest.
    let events: Vec<Event> = (0..21)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();

    let sent = contents(&context.build(view(&events)).unwrap().messages);

    let index = &sent[0];
    assert!(index.contains("user: p11"));
    assert!(index.contains("user: p10"));
    assert!(!index.contains("user: p9"));
    assert_eq!(first_word(&sent[1]), "p12");
}

#[test]
fn percept_s_own_prompt_is_neither_indexed_nor_charged_to_the_window() {
    let context = history_then_turn(WINDOW, 200);
    // A reflect prompt of two hundred tokens would overflow the window
    // on its own; charged at nothing, the ten prompts around it all
    // stay in, and it appears in neither the index nor the history.
    let mut events: Vec<Event> = (0..5)
        .map(|i| prompt(Actor::User, &format!("p{i}"), 10))
        .collect();
    events.push(prompt(Actor::System, "revise the maps", 200));
    events.extend((5..10).map(|i| prompt(Actor::User, &format!("p{i}"), 10)));

    let sent = contents(&context.build(view(&events)).unwrap().messages);

    assert_eq!(sent.len(), 10);
    assert_eq!(first_word(&sent[0]), "p0");
    assert!(sent.iter().all(|message| !message.contains("revise")));
}
