use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopRoundView {
    pub loop_id: String,
    pub round: u32,
    #[serde(default)]
    pub item: Option<serde_json::Value>,
    #[serde(default)]
    pub failed: bool,
    /// Zero-based position in the iteration sequence, when recorded.
    #[serde(default)]
    pub iteration: Option<u64>,
    #[serde(default)]
    pub max_iterations: Option<u64>,
    /// Cumulative failures for this loop, mirroring the engine counters.
    #[serde(default)]
    pub failures: u32,
    /// Failure policy in effect (for example `continue` or `fail_fast`).
    #[serde(default)]
    pub policy: Option<String>,
    /// True when the round ran after a checkpoint restore.
    #[serde(default)]
    pub resumed: bool,
    /// Nodes of the current round already completed before this record.
    #[serde(default)]
    pub completed_nodes: Vec<String>,
    /// Iteration source: `count` for counted loops, `items` for iterables.
    #[serde(default)]
    pub iterable_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeBranchView {
    pub branch_id: String,
    pub success: bool,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeView {
    pub join_node_id: String,
    #[serde(default)]
    pub branch_count: usize,
    #[serde(default)]
    pub failures_absorbed: usize,
    #[serde(default)]
    pub summary: serde_json::Value,
    /// Join outcome: `success`, `partial` or `failed`. When absent the merge
    /// analyzer derives it from the branch records.
    #[serde(default)]
    pub outcome: Option<String>,
    /// Failure policy in effect (for example `fail_fast` or `threshold`).
    #[serde(default)]
    pub policy: Option<String>,
    #[serde(default)]
    pub branches: Vec<MergeBranchView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteDecisionPoint {
    pub node_id: String,
    #[serde(default)]
    pub branches: Vec<RouteBranch>,
    #[serde(default)]
    pub default_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RouteBranch {
    pub target_node_id: String,
    #[serde(default)]
    pub expression: Option<String>,
}
