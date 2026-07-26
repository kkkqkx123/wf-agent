use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionStartedEvent {
    pub base: super::BaseEvent,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionCompletedEvent {
    pub base: super::BaseEvent,
    pub output: serde_json::Value,
    pub execution_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionFailedEvent {
    pub base: super::BaseEvent,
    pub error: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionPausedEvent {
    pub base: super::BaseEvent,
    pub reason: Option<String>,
    pub node_id: Option<String>,
    pub completed_nodes: Option<u32>,
    pub pending_tools_cancelled: Option<bool>,
    pub checkpoint_created: Option<bool>,
    pub checkpoint_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionResumedEvent {
    pub base: super::BaseEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionCancelledEvent {
    pub base: super::BaseEvent,
    pub reason: Option<String>,
    pub node_id: Option<String>,
    pub completed_nodes: Option<u32>,
    pub pending_tools_cancelled: Option<bool>,
    pub checkpoint_created: Option<bool>,
    pub checkpoint_id: Option<String>,
    pub pause_duration: Option<i64>,
    pub max_pause_duration: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionStateChangedEvent {
    pub base: super::BaseEvent,
    pub previous_status: String,
    pub new_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionForkStartedEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub fork_config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionForkCompletedEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub child_execution_ids: Vec<super::super::Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionJoinStartedEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub child_execution_ids: Vec<super::super::Id>,
    pub join_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionJoinConditionMetEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub child_execution_ids: Vec<super::super::Id>,
    pub condition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionJoinCompletedEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub child_execution_ids: Vec<super::super::Id>,
    pub join_strategy: String,
    pub aggregated_output_count: u32,
    pub duration: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionJoinFailedEvent {
    pub base: super::BaseEvent,
    pub parent_execution_id: super::super::Id,
    pub child_execution_ids: Vec<super::super::Id>,
    pub join_strategy: String,
    pub error: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionCopyStartedEvent {
    pub base: super::BaseEvent,
    pub source_execution_id: super::super::Id,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowExecutionCopyCompletedEvent {
    pub base: super::BaseEvent,
    pub source_execution_id: super::super::Id,
    pub copied_execution_id: super::super::Id,
}
