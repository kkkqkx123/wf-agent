use wf_types::message::Message;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub token_usage: u64,
}

pub struct ConversationSession {
    pub state: ConversationState,
}

impl Default for ConversationSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationSession {
    pub fn new() -> Self {
        Self {
            state: ConversationState {
                messages: Vec::new(),
                token_usage: 0,
            },
        }
    }

    pub fn add_message(&mut self, message: Message) {
        self.state.messages.push(message);
    }

    pub fn messages(&self) -> &[Message] {
        &self.state.messages
    }

    pub fn token_usage(&self) -> u64 {
        self.state.token_usage
    }

    pub fn add_token_usage(&mut self, tokens: u64) {
        self.state.token_usage += tokens;
    }
}
