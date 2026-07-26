use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallStartedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub tool_id: super::super::Id,
    pub task_id: Option<String>,
    pub batch_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallCompletedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub tool_id: super::super::Id,
    pub task_id: Option<String>,
    pub batch_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_result: serde_json::Value,
    pub execution_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallFailedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub tool_id: super::super::Id,
    pub task_id: Option<String>,
    pub batch_id: Option<String>,
    pub tool_name: Option<String>,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallBlockedEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub node_id: super::super::Id,
    pub tool_id: super::super::Id,
    pub tool_name: Option<String>,
    pub failure_count: u32,
    pub last_error: Option<String>,
    pub remaining_cooldown: Option<i64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAddedEvent {
    pub base: super::BaseEvent,
    pub node_id: super::super::Id,
    pub tool_ids: Vec<super::super::Id>,
    pub scope: String,
    pub added_count: u32,
    pub skipped_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolVisibilityChangedEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub workflow_id: Option<super::super::Id>,
    pub node_id: Option<super::super::Id>,
    pub scope: String,
    pub scope_id: super::super::Id,
    pub change_type: String,
    pub visible_tool_ids: Vec<super::super::Id>,
    pub previous_visible_tool_ids: Option<Vec<super::super::Id>>,
    pub timestamp: i64,
}
