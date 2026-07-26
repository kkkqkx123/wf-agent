use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStartedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCompletedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub output: serde_json::Value,
    pub execution_time: super::super::Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeFailedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub error: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeCustomEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub node_type: String,
    pub event_name: String,
    pub event_data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkStartedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub branch_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkBranchStartedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub fork_path_id: String,
    pub branch_execution_id: super::super::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkBranchCompletedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub fork_path_id: String,
    pub branch_execution_id: super::super::Id,
    pub status: String,
    pub execution_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ForkCompletedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub total_branches: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub total_execution_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSyncStartedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub source_path_id: super::super::Id,
    pub parent_execution_id: super::super::Id,
    pub target_path_id: Option<super::super::Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSyncCompletedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub source_path_id: super::super::Id,
    pub parent_execution_id: super::super::Id,
    pub variable_count: u32,
    pub data_count: u32,
    pub message_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeSyncFailedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub source_path_id: super::super::Id,
    pub parent_execution_id: super::super::Id,
    pub error: String,
}
