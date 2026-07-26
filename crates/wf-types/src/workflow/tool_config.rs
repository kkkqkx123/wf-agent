use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AvailableTools {
    pub tool_ids: Vec<String>,
    pub allow_all: Option<bool>,
}
