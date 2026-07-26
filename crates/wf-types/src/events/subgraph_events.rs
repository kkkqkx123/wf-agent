use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphStartedEvent {
    pub base: super::BaseEvent,
    pub subgraph_id: super::super::Id,
    pub parent_workflow_id: super::super::Id,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubgraphCompletedEvent {
    pub base: super::BaseEvent,
    pub subgraph_id: super::super::Id,
    pub output: serde_json::Value,
    pub execution_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubgraphStartedEvent {
    pub base: super::BaseEvent,
    pub subgraph_id: super::super::Id,
    pub trigger_id: super::super::Id,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubgraphCompletedEvent {
    pub base: super::BaseEvent,
    pub subgraph_id: super::super::Id,
    pub trigger_id: super::super::Id,
    pub output: Option<serde_json::Value>,
    pub execution_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggeredSubgraphFailedEvent {
    pub base: super::BaseEvent,
    pub subgraph_id: super::super::Id,
    pub trigger_id: super::super::Id,
    pub error: String,
    pub execution_time: Option<i64>,
}
