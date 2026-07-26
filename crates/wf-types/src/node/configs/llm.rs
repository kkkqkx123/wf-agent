use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmNodeConfig {
    pub llm_profile_id: String,
    pub system_prompt: Option<String>,
    pub user_prompt: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub output_variable: Option<String>,
}
