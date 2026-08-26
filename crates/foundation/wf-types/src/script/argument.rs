use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptArgument {
    pub name: String,
    pub value: serde_json::Value,
    pub r#type: Option<String>,
}
