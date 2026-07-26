use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopStartNodeConfig {
    pub max_iterations: Option<u32>,
    pub condition: Option<String>,
    pub continue_on_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopEndNodeConfig {
    pub loop_start_node_id: String,
}
