use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStartedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub max_iterations: u32,
    pub initial_message_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCompletedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iterations: u32,
    pub tool_call_count: u32,
    pub success: bool,
    pub error: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnStartedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentTurnCompletedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub should_continue: bool,
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageStartedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentMessageCompletedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolExecutionStartedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub tool_call_id: super::super::Id,
    pub tool_name: String,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentToolExecutionCompletedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub tool_call_id: super::super::Id,
    pub tool_name: String,
    pub success: bool,
    pub duration: Option<i64>,
    pub error: Option<serde_json::Value>,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentIterationStartedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentIterationCompletedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub tool_call_count: u32,
    pub should_continue: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentHookTriggeredEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub agent_loop_entity_id: super::super::Id,
    pub hook_type: String,
    pub event_name: String,
    pub event_data: HashMap<String, serde_json::Value>,
    pub iteration: u32,
    pub parent_context: Option<AgentParentContext>,
    pub metadata: Option<super::super::Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentParentContext {
    pub parent_type: String,
    pub parent_id: String,
    pub node_id: Option<String>,
    pub delegation_purpose: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentPausedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub tool_call_count: u32,
    pub is_streaming: bool,
    pub pending_tool_calls: u32,
    pub stream_message_preserved: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentCancelledEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub tool_call_count: u32,
    pub is_streaming: bool,
    pub pending_tool_calls: u32,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentResumedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub tool_call_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentFailedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub tool_call_count: u32,
    pub error: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentSteeringInjectedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentFollowupQueuedEvent {
    pub base: super::BaseEvent,
    pub agent_loop_id: super::super::Id,
    pub iteration: u32,
    pub followup_text: Option<String>,
}
