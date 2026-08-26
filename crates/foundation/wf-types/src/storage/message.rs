use serde::{Deserialize, Serialize};

/// Persisted message record: a conversation message scoped to a workflow
/// execution or an agent loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageStorageMetadata {
    pub id: super::super::Id,
    /// Owning workflow execution id (`agent_loop_id` is set for agent loops).
    pub execution_id: super::super::Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_loop_id: Option<super::super::Id>,
    /// The full message payload.
    pub message: super::super::message::Message,
}
