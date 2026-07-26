use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressiveToolExecutionStartEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub node_id: Option<super::super::Id>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub batch_id: Option<String>,
    pub tool_index: Option<u32>,
    pub total_tools: Option<u32>,
    pub pending_queue: Option<Vec<PendingToolCallInfo>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingToolCallInfo {
    pub tool_call_id: String,
    pub tool_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProgressiveToolExecutionEndEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub node_id: Option<super::super::Id>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub batch_id: Option<String>,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub execution_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolQueueUpdateEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub node_id: Option<super::super::Id>,
    pub batch_id: String,
    pub completed_count: u32,
    pub total_count: u32,
    pub pending_queue: Vec<PendingToolCallInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalAnnotatedEvent {
    pub base: super::BaseEvent,
    pub execution_id: super::super::Id,
    pub node_id: Option<super::super::Id>,
    pub interaction_id: super::super::Id,
    pub tool_call_id: String,
    pub tool_name: String,
    pub annotation: String,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalRequestedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub tool_call: serde_json::Value,
    pub tool_description: Option<String>,
    pub context_id: super::super::Id,
    pub node_id: Option<super::super::Id>,
    pub batch_id: Option<String>,
    pub tool_index: Option<u32>,
    pub total_tools: Option<u32>,
    pub pending_queue: Option<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalRespondedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub result: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolApprovalFailedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionRequestedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub questions: Vec<QuestionData>,
    pub additional_info_label: String,
    pub timeout: i64,
    pub metadata: Option<FollowupQuestionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionData {
    pub index: u32,
    pub text: String,
    pub options: Vec<QuestionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionOption {
    pub index: u32,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionMetadata {
    pub execution_id: Option<super::super::Id>,
    pub node_id: Option<super::super::Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionRespondedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub answers: Vec<QuestionAnswer>,
    pub additional_info: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestionAnswer {
    pub question_index: u32,
    pub selected_option_index: u32,
    pub custom_input: Option<String>,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowupQuestionFailedEvent {
    pub base: super::BaseEvent,
    pub interaction_id: super::super::Id,
    pub error: String,
}
