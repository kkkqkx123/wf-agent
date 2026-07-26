use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkNodeConfig {
    pub branches: Vec<ForkBranch>,
    pub join_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkBranch {
    pub id: String,
    pub node_ids: Vec<String>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinNodeConfig {
    pub join_type: String,
    pub condition: Option<String>,
    pub timeout: Option<u64>,
}
