use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointStateBase {
    pub id: super::super::Id,
    pub workflow_id: Option<super::super::Id>,
    pub execution_id: Option<super::super::Id>,
    pub timestamp: super::super::Timestamp,
    pub format_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CheckpointTrigger {
    Manual,
    Auto,
    OnError,
    OnPause,
    Hook,
}
