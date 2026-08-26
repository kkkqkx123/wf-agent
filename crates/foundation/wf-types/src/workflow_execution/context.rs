use serde::{Deserialize, Serialize};

use crate::Id;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkJoinContext {
    pub fork_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_path_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubworkflowContext {
    pub parent_execution_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_execution_ids: Option<Vec<Id>>,
    pub triggered_subworkflow_id: String,
}
