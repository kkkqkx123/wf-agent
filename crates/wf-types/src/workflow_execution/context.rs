use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkJoinContext {
    pub fork_node_id: String,
    pub branch_id: Option<String>,
    pub parent_execution_id: Option<super::super::Id>,
}
