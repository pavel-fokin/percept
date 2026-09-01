use std::time::Duration;

use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::percept::{Message, Model, ReplyStream};

/// What Stub streams back, regardless of input - static text for now,
/// standing in for a real model's generated tokens.
const STATIC_REPLY: &str =
    "This is a simulated streaming reply. Words appear one at a time, just like a real language model would send them.";

/// Streams STATIC_REPLY word by word: an initial random 0.5-1.5s delay
/// (time to "first token"), then ~40-120ms between words - long enough,
/// at both points, to make the streaming actually visible.
pub struct Stub;

impl Model for Stub {
    fn reply(&self, _messages: &[Message]) -> ReplyStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(rand::random_range(500..1500))).await;

            for (i, word) in STATIC_REPLY.split_whitespace().enumerate() {
                let chunk = if i == 0 {
                    word.to_string()
                } else {
                    format!(" {word}")
                };
                if tx.send(Ok(chunk)).is_err() {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(rand::random_range(40..120))).await;
            }
        });

        Box::pin(UnboundedReceiverStream::new(rx))
    }
}
