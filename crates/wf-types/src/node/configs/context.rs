use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextProcessorNodeConfig {
    pub include_variables: Option<bool>,
    pub include_messages: Option<bool>,
    pub max_context_tokens: Option<u32>,
    pub template: Option<String>,
}
