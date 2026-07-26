use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerExecutionResult {
    pub triggered: bool,
    pub execution_id: Option<super::super::Id>,
    pub error: Option<String>,
}
