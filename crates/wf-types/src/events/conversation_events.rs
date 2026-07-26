use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageAddedEvent {
    pub base: super::BaseEvent,
    pub node_id: Option<super::super::Id>,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationStateChangedEvent {
    pub base: super::BaseEvent,
    pub node_id: Option<super::super::Id>,
    pub message_count: u32,
    pub token_usage: u32,
}
