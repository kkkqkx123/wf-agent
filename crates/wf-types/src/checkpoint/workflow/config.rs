use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowCheckpointConfig {
    pub enabled: bool,
    pub interval_nodes: Option<u32>,
    pub on_error: Option<bool>,
    pub on_completion: Option<bool>,
}
