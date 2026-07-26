use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariableOperationConfig {
    pub operation: String,
    pub source_variable: String,
    pub target_variable: String,
    pub transform: Option<String>,
}
