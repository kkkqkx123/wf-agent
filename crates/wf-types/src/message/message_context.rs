use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageContext {
    pub messages: Vec<super::Message>,
    pub max_messages: Option<u32>,
    pub max_tokens: Option<u32>,
}
