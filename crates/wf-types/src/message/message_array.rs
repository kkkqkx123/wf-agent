use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageArrayState {
    pub messages: Vec<super::Message>,
    pub total_tokens: Option<u32>,
    pub truncated: Option<bool>,
}
