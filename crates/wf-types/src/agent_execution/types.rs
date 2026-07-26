use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentLoopStatus {
    Created,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub start_time: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<super::super::Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IterationRecord {
    pub iteration: u32,
    pub start_time: super::super::Timestamp,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<super::super::Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallRecord>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentLoopResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub iterations: u32,
    pub tool_call_count: u32,
    pub iteration_level_retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_data: Option<serde_json::Value>,
}
