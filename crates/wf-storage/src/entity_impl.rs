use serde_json::Value;

use crate::domain::entity::Entity;

impl Entity for wf_types::WorkflowDefinition {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "workflow"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "type": self.r#type.as_ref().map(|t| format!("{:?}", t)),
        })
    }
}

impl Entity for wf_types::WorkflowExecution {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "execution"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "status": self.status,
            "workflowId": self.workflow_id,
        })
    }
}

impl Entity for wf_types::storage::checkpoint::CheckpointStorageMetadata {
    type Metadata = Value;

    #[allow(clippy::misnamed_getters)]
    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "checkpoint"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "entityId": self.entity_id,
            "checkpointType": self.checkpoint_type,
            "timestamp": self.timestamp,
            "status": self.status,
        })
    }
}

impl Entity for wf_types::TaskStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "task"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "taskType": self.task_type,
            "status": self.status,
            "createdAt": self.created_at,
            "updatedAt": self.updated_at,
        })
    }
}

impl Entity for wf_types::AgentLoopStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "agent_loop"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "status": self.status,
            "currentIteration": self.current_iteration,
        })
    }
}

impl Entity for wf_types::FileCheckpointStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "file_checkpoint"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "filePath": self.file_path,
            "checkpointId": self.checkpoint_id,
            "sizeBytes": self.size_bytes,
        })
    }
}
