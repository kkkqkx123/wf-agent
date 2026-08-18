use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ForkStrategy {
    Serial,
    Parallel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkPath {
    pub path_id: String,
    pub child_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkNodeConfig {
    pub fork_paths: Vec<ForkPath>,
    pub fork_strategy: ForkStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failed_branches: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_execution_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_branch_timeout: Option<u64>,
    /// Whether the fork handler waits for every branch to settle before
    /// returning (blocking, default). `false` launches the branches and
    /// returns immediately; the JOIN node then waits for the branches via
    /// the fork registry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for_completion: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkNodeOutput {
    pub launched_branches: Vec<LaunchedBranch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchedBranch {
    pub path_id: String,
    pub child_node_id: String,
    pub strategy: ForkStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JoinStrategy {
    WaitForAll,
    WaitForAny,
    WaitForN,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinNodeConfig {
    pub fork_path_ids: Vec<String>,
    pub join_strategy: JoinStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_path_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JoinNodeOutput {
    pub completed_branches: Vec<String>,
    pub failed_branches: Vec<String>,
    pub skipped_branches: Vec<String>,
    pub strategy: JoinStrategy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregated_output: Option<serde_json::Value>,
}
