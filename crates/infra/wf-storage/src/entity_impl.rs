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
/// The hierarchy is stored forward-only as one materialised path: an execution
/// records where it sits, and everything its path can answer — root, depth,
/// ancestors, direct parent — is read off that path rather than copied
/// alongside it, so no pair of indexed keys can describe a different tree. The
/// engines that own the parent and the root cannot be derived from a path of
/// ids, so those two are carried as their own keys. A root execution therefore
/// leaves `parentExecutionId` absent (never pointing at itself) and reports
/// itself as its own root.
///
/// `startedAt` carries the run start under its own name rather than sharing
/// `timestamp`: checkpoint and message rows use that key for the time of
/// their own event, so one range filter over `timestamp` would mean a
/// different window on an execution row than on a checkpoint row.
fn execution_metadata(
    hierarchy: Option<&wf_types::execution::ExecutionHierarchy>,
    execution_id: &str,
    started_at: i64,
    own: &[(&str, Value)],
) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("startedAt".into(), json!(started_at));
    match hierarchy {
        Some(h) => {
            map.insert("executionPath".into(), json!(h.path()));
            if let Some(parent) = h.parent_execution_id() {
                map.insert("parentExecutionId".into(), json!(parent));
                if let Some(kind) = &h.parent_execution_type {
                    map.insert("parentExecutionType".into(), json!(kind));
                }
            }
            // Which engine owns the root is a fact about the root, not about
            // this record, so the path cannot yield it: it is carried alongside
            // the path so a hierarchy read answers without fetching the root's
            // own record.
            if let Some(kind) = &h.root_execution_type {
                map.insert("rootExecutionType".into(), json!(kind));
            }
        }
        None => {
            map.insert(
                "executionPath".into(),
                json!(wf_types::execution::encode_path(
                    &[execution_id.to_string()]
                )),
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
        self.metadata_document()
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

#[cfg(test)]
mod tests {
    use super::*;
    use wf_types::execution::{ExecutionHierarchy, ExecutionType};
    use wf_types::Id;

    /// The stored keys are the contract the index reader parses: the facts it
    /// reads have to be present on every row, and the facts it derives from
    /// the path must not linger beside it where they could contradict it.
    #[test]
    fn execution_metadata_carries_the_indexed_facts_only() {
        let hierarchy = ExecutionHierarchy::new(
            Id::from("wf-1".to_string()),
            Id::from("kid".to_string()),
            vec![Id::from("root".to_string())],
            Some(ExecutionType::Workflow),
            Some(ExecutionType::AgentLoop),
            None,
        );
        let meta = execution_metadata(
            Some(&hierarchy),
            "kid",
            1_700_000_000,
            &[("status", json!("failed"))],
        );

        assert_eq!(meta["executionPath"], json!("/root/kid/"));
        assert_eq!(meta["parentExecutionId"], json!("root"));
        assert_eq!(meta["parentExecutionType"], json!("workflow"));
        assert_eq!(meta["rootExecutionType"], json!("agent_loop"));
        assert_eq!(meta["startedAt"], json!(1_700_000_000));
        assert_eq!(meta["status"], json!("failed"));

        // Both are read off the path now; storing them would let a row
        // contradict the path it sits on.
        assert!(meta.get("depth").is_none());
        assert!(meta.get("rootExecutionId").is_none());
        // The run start owns `startedAt`; `timestamp` stays the event time of
        // the other entities that share this metadata layout.
        assert!(meta.get("timestamp").is_none());
    }

    /// A root has no parent, so neither does its record, and it still carries
    /// the path that puts it and its descendants under one prefix.
    #[test]
    fn a_root_record_keeps_its_path_without_a_parent() {
        let hierarchy = ExecutionHierarchy::new(
            Id::from("wf-1".to_string()),
            Id::from("root".to_string()),
            vec![],
            None,
            Some(ExecutionType::Workflow),
            None,
        );
        let meta = execution_metadata(Some(&hierarchy), "root", 7, &[]);

        assert_eq!(meta["executionPath"], json!("/root/"));
        assert!(meta.get("parentExecutionId").is_none());
        assert!(meta.get("parentExecutionType").is_none());
        assert_eq!(meta["rootExecutionType"], json!("workflow"));
        assert_eq!(meta["startedAt"], json!(7));
    }

    /// Without a lineage the record still states where it sits, so a row
    /// written before any link exists is readable by the same parser.
    #[test]
    fn a_record_without_lineage_states_its_own_path() {
        let meta = execution_metadata(None, "standalone", 7, &[]);
        assert_eq!(meta["executionPath"], json!("/standalone/"));
        assert!(meta.get("parentExecutionId").is_none());
    }
}
