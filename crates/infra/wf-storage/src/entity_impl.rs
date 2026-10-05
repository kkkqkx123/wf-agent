use serde_json::{json, Value};

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

impl Entity for wf_types::agent::AgentDefinition {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "agent_definition"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
        })
    }
}

impl Entity for wf_types::agent::AgentTemplate {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "agent_template"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
        })
    }
}

/// Metadata keys shared by both execution record types.
///
/// The hierarchy is stored forward-only: an execution records its own parent
/// and root, and the shape of the tree is recovered by querying those fields.
/// A root execution therefore leaves `parentExecutionId` absent (never
/// pointing at itself, so "children of X" can never match X) and reports
/// itself as its own root.
///
/// `executionPath` is the materialised ancestor chain. It is the only field
/// that fully describes where a record sits, so the root and the depth are
/// derived from it rather than copied alongside it: a row whose fields cannot
/// all agree is then impossible to write.
///
/// `timestamp` carries the run start, matching the key the checkpoint records
/// already use for the same notion, so one index serves both.
fn execution_metadata(
    hierarchy: Option<&wf_types::execution::ExecutionHierarchy>,
    execution_id: &str,
    execution_kind: &str,
    started_at: i64,
    own: &[(&str, Value)],
) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("executionKind".into(), json!(execution_kind));
    map.insert("timestamp".into(), json!(started_at));
    match hierarchy {
        Some(h) => {
            let chain = h.chain();
            map.insert("executionPath".into(), json!(h.path()));
            map.insert("depth".into(), json!(chain.len() as u32 - 1));
            map.insert(
                "rootExecutionId".into(),
                json!(chain.first().map_or(execution_id, String::as_str)),
            );
            if let Some(parent) = h.parent_execution_id.as_ref() {
                map.insert("parentExecutionId".into(), json!(parent));
            }
        }
        None => {
            map.insert("rootExecutionId".into(), json!(execution_id));
            map.insert("depth".into(), json!(0));
            map.insert(
                "executionPath".into(),
                json!(wf_types::execution::encode_path(&[execution_id
                    .to_string()])),
            );
        }
    }
    for (key, value) in own {
        map.insert((*key).to_string(), value.clone());
    }
    Value::Object(map)
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
        execution_metadata(
            self.hierarchy.as_ref(),
            &self.id,
            "workflow",
            self.started_at,
            &[
                ("status", json!(self.status)),
                ("workflowId", json!(self.workflow_id)),
            ],
        )
    }
}

impl Entity for wf_types::storage::checkpoint::CheckpointStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        self.id.as_str()
    }

    fn entity_type() -> &'static str {
        "checkpoint"
    }

    fn metadata(&self) -> Self::Metadata {
        // The record-level `entity_type` (`checkpoint` for workflow
        // executions, `agent_loop` for agent loops) overrides the static
        // adapter type: `EntityStore` merges record metadata over the base
        // map, so domain filters (`list_by_entity`, `entity_type_filter`)
        // match the owning domain instead of every checkpoint row.
        serde_json::json!({
            "entityType": self.entity_type,
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
            "executionId": self.execution_id,
            "instanceId": self.instance_id,
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
        execution_metadata(
            self.hierarchy.as_ref(),
            &self.id,
            "agent_loop",
            self.started_at,
            &[
                ("definitionId", json!(self.definition_id)),
                ("status", json!(self.status)),
                ("currentIteration", json!(self.current_iteration)),
                ("toolCallCount", json!(self.tool_call_count)),
            ],
        )
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

impl Entity for wf_types::TriggerTemplateStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "trigger_template"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "name": self.name,
            "triggerType": self.trigger_type,
            "category": self.category,
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
            "outcome": self.outcome.as_str(),
        })
    }
}

impl Entity for wf_types::MessageStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "message"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "executionId": self.execution_id,
            "agentLoopId": self.agent_loop_id,
            "role": self.message.role,
            "timestamp": self.message.timestamp,
        })
    }
}

impl Entity for wf_types::VariableStorageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "variable"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "executionId": self.execution_id,
            "scope": self.scope,
            "name": self.name,
            "updatedAt": self.updated_at,
        })
    }
}

impl Entity for wf_types::TemplateUsageMetadata {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "template_usage"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "templateId": self.template_id,
            "kind": self.kind,
            "count": self.count,
            "updatedAt": self.updated_at,
        })
    }
}

impl Entity for wf_types::tool::Tool {
    type Metadata = Value;

    fn entity_id(&self) -> &str {
        &self.id
    }

    fn entity_type() -> &'static str {
        "tool_definition"
    }

    fn metadata(&self) -> Self::Metadata {
        serde_json::json!({
            "toolId": self.id,
            "name": self.name,
            "toolType": serde_json::to_value(&self.tool_type).ok(),
            "enabled": self.enabled.unwrap_or(true),
        })
    }
}
