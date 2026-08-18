use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolType {
    Stateless,
    Stateful,
    Rest,
    BuiltIn,
    Mcp,
}
