use std::time::Duration;

use crate::percept::{Message, Model};

/// What Stub streams back, regardless of input - static text for now,
/// standing in for a real model's generated tokens.
const STATIC_REPLY: &str =
    "This is a simulated streaming reply. Words appear one at a time, just like a real language model would send them.";

/// Streams STATIC_REPLY word by word: an initial random 0.5-1.5s delay
/// (time to "first token"), then ~40-120ms between words - long enough,
/// at both points, to make the streaming actually visible.
pub struct Stub;

impl Model for Stub {
    fn reply(
        &self,
        _messages: &[Message],
        on_chunk: &mut dyn FnMut(String),
    ) -> Result<(), Box<dyn std::error::Error>> {
        std::thread::sleep(Duration::from_millis(rand::random_range(500..1500)));

        for (i, word) in STATIC_REPLY.split_whitespace().enumerate() {
            on_chunk(if i == 0 {
                word.to_string()
            } else {
                format!(" {word}")
            });
            std::thread::sleep(Duration::from_millis(rand::random_range(40..120)));
        }
        Ok(())
    }
}
