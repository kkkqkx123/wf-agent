use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InteractiveScriptConfig {
    pub enabled: bool,
    pub timeout_seconds: Option<u64>,
    pub allow_user_input: Option<bool>,
}
