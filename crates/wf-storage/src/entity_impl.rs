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
            "type": self.r#type.as_ref().and_then(|t| serde_json::to_value(t).ok()),
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

impl Entity for wf_types::AgentExecution {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "agent_execution"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "definitionId": self.definition_id,
            "status": self.status,
            "currentIteration": self.current_iteration,
            "toolCallCount": self.tool_call_count,
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

impl Entity for wf_types::TriggerStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "trigger"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "event": self.event,
            "enabled": self.enabled,
        })
    }
}

impl Entity for wf_types::ToolStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "tool"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "toolId": self.tool_id,
            "toolType": self.tool_type,
            "enabled": self.enabled,
        })
    }
}

impl Entity for wf_types::ScriptStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "script"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "language": self.language,
            "enabled": self.enabled,
        })
    }
}

impl Entity for wf_types::NodeTemplateStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "node_template"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "nodeType": self.node_type,
        })
    }
}

impl Entity for wf_types::HookTemplateStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "hook_template"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "hookType": self.hook_type,
        })
    }
}

impl Entity for wf_types::AgentProfileStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "agent_profile"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "profileId": self.profile_id,
            "name": self.name,
        })
    }
}

impl Entity for wf_types::UserInteractionStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "user_interaction"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "executionId": self.execution_id,
            "interactionType": self.interaction_type,
            "status": self.status,
        })
    }
}

impl Entity for wf_types::TriggerExecutionStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "trigger_execution"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "triggerName": self.trigger_name,
            "triggerType": self.trigger_type,
            "event": self.event,
            "executionId": self.execution_id,
            "workflowId": self.workflow_id,
            "triggeredAt": self.triggered_at,
            "success": self.success,
        })
    }
}

impl Entity for wf_types::FileCheckpointStorageMetadata {
    type Metadata = Value;

    #[allow(clippy::misnamed_getters)]
    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "file_checkpoint"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "entityId": self.entity_id,
            "filePath": self.file_path,
            "checkpointId": self.checkpoint_id,
            "sizeBytes": self.size_bytes,
        })
    }
}
