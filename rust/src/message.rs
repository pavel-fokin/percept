#[derive(Clone, Copy, PartialEq)]
pub enum Sender {
    User,
    Assistant,
}

pub struct ChatMessage {
    pub from: Sender,
    pub text: String,
}

pub fn stub_assistant_reply(user_text: &str) -> String {
    format!("You said: {user_text}")
}
