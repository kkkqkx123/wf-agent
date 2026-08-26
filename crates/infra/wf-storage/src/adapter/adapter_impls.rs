use serde_json::Value;
use std::collections::HashMap;

use crate::adapter::agent_execution::{AgentExecutionListOptions, AgentExecutionStorageAdapter};
use crate::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use crate::adapter::agent_profile::{AgentProfileListOptions, AgentProfileStorageAdapter};
use crate::adapter::base::BaseStorageAdapter;
use crate::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use crate::adapter::execution::{WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter};
use crate::adapter::message::{MessageListOptions, MessageStorageAdapter};
use crate::adapter::metrics::{MetricRecord, MetricsDataPoint, MetricsStorageAdapter};
use crate::adapter::node_template::{NodeTemplateListOptions, NodeTemplateStorageAdapter};
use crate::adapter::script::{ScriptListOptions, ScriptStorageAdapter};
use crate::adapter::task::{TaskListOptions, TaskStorageAdapter};
use crate::adapter::tool::{ToolListOptions, ToolStorageAdapter};
use crate::adapter::tool_definition::{ToolDefinitionListOptions, ToolDefinitionStorageAdapter};
use crate::adapter::trigger::{TriggerListOptions, TriggerStorageAdapter};
use crate::adapter::trigger_execution::{
    TriggerExecutionListOptions, TriggerExecutionStorageAdapter,
};
use crate::adapter::trigger_template::{TriggerTemplateListOptions, TriggerTemplateStorageAdapter};
use crate::adapter::user_interaction::{UserInteractionListOptions, UserInteractionStorageAdapter};
use crate::adapter::variable::{VariableListOptions, VariableStorageAdapter};
use crate::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use crate::domain::store::{BatchStore, QueryFilter, Store};
use crate::error::StorageError;
use crate::make_base_adapter;
use crate::store::entity_store::EntityStore;

// ─── Macro invocation: generates BaseStorageAdapter impl + struct ───

make_base_adapter!(
    WorkflowStorage,
    wf_types::WorkflowDefinition,
    WorkflowListOptions
);
make_base_adapter!(
    WorkflowExecutionStorage,
    wf_types::WorkflowExecution,
    WorkflowExecutionListOptions
);
make_base_adapter!(
    CheckpointStorage,
    wf_types::Checkpoint,
    CheckpointListOptions
);
make_base_adapter!(TaskStorage, wf_types::TaskStorageMetadata, TaskListOptions);
make_base_adapter!(
    AgentLoopStorage,
    wf_types::AgentLoopStorageMetadata,
    AgentLoopListOptions
);
make_base_adapter!(
    AgentExecutionStorage,
    wf_types::AgentExecution,
    AgentExecutionListOptions
);
make_base_adapter!(
    TriggerStorage,
    wf_types::TriggerStorageMetadata,
    TriggerListOptions
);
make_base_adapter!(ToolStorage, wf_types::ToolStorageMetadata, ToolListOptions);
make_base_adapter!(
    ToolDefinitionStorage,
    wf_types::tool::Tool,
    ToolDefinitionListOptions
);
make_base_adapter!(
    ScriptStorage,
    wf_types::ScriptStorageMetadata,
    ScriptListOptions
);
make_base_adapter!(
    NodeTemplateStorage,
    wf_types::NodeTemplateStorageMetadata,
    NodeTemplateListOptions
);
make_base_adapter!(
    AgentProfileStorage,
    wf_types::AgentProfileStorageMetadata,
    AgentProfileListOptions
);
make_base_adapter!(
    TriggerTemplateStorage,
    wf_types::TriggerTemplateStorageMetadata,
    TriggerTemplateListOptions
);
make_base_adapter!(
    UserInteractionStorage,
    wf_types::UserInteractionStorageMetadata,
    UserInteractionListOptions
);
make_base_adapter!(
    TriggerExecutionStorage,
    wf_types::TriggerExecutionStorageMetadata,
    TriggerExecutionListOptions
);
make_base_adapter!(
    MessageStorage,
    wf_types::MessageStorageMetadata,
    MessageListOptions
);
make_base_adapter!(
    VariableStorage,
    wf_types::VariableStorageMetadata,
    VariableListOptions
);

// ─── WorkflowStorageAdapter ───

impl<S: Store> WorkflowStorageAdapter for WorkflowStorage<S> {
    async fn update_metadata(
        &self,
        id: &str,
        metadata: &HashMap<String, Value>,
    ) -> Result<(), StorageError> {
        if let Some(data) = self.store().load(id).await? {
            let mut full_meta = data.1;
            if let Some(obj) = full_meta.as_object_mut() {
                for (k, v) in metadata {
                    obj.insert(k.clone(), v.clone());
                }
            }
            self.store().save(id, &data.0, &full_meta).await?;
        }
        Ok(())
    }

    async fn save_version(
        &self,
        workflow_id: &str,
        version: &str,
        template: &wf_types::WorkflowDefinition,
    ) -> Result<(), StorageError> {
        let composite_id = format!("{}:v{}", workflow_id, version);
        let data = serde_json::to_vec(template)?;
        let metadata = serde_json::json!({
            "entityType": <wf_types::WorkflowDefinition as crate::domain::Entity>::entity_type(),
            "workflowId": workflow_id,
            "version": version,
            "compressed": false,
            "name": template.name,
        });
        self.store().save(&composite_id, &data, &metadata).await
    }

    async fn list_versions(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::WorkflowDefinition>, StorageError> {
        let prefix = format!("{}:v", workflow_id);
        let filter = QueryFilter::new().with_id_prefix(&prefix);
        self.entity_store.list(Some(&filter)).await
    }

    async fn load_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<Option<wf_types::WorkflowDefinition>, StorageError> {
        let composite_id = format!("{}:v{}", workflow_id, version);
        self.entity_store.load(&composite_id).await
    }

    async fn delete_version(&self, workflow_id: &str, version: &str) -> Result<bool, StorageError> {
        let composite_id = format!("{}:v{}", workflow_id, version);
        let existed = self.entity_store.exists(&composite_id).await?;
        self.entity_store.delete(&composite_id).await?;
        Ok(existed)
    }
}

// ─── WorkflowExecutionStorageAdapter ───

impl<S: Store> WorkflowExecutionStorageAdapter for WorkflowExecutionStorage<S> {
    async fn update_status(
        &self,
        id: &str,
        status: &wf_types::ExecutionStatus,
    ) -> Result<(), StorageError> {
        if let Some(mut entity) = self.entity_store.load(id).await? {
            entity.status = status.clone();
            self.entity_store.save(&entity).await?;
        }
        Ok(())
    }
}

// ─── CheckpointStorageAdapter ───

impl<S: Store> CheckpointStorageAdapter for CheckpointStorage<S> {
    async fn list_by_entities_with_metadata(
        &self,
        entity_ids: &[String],
        entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let mut results = Vec::new();
        for eid in entity_ids {
            let entries = self.list_by_entity(eid, entity_type).await?;
            results.extend(entries);
        }
        Ok(results)
    }

    async fn list_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let filter = QueryFilter::new()
            .with_field("entityId", entity_id)
            .with_entity_type(entity_type);
        self.entity_store.list(Some(&filter)).await
    }

    async fn get_latest_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Option<wf_types::Checkpoint>, StorageError> {
        let filter = QueryFilter::new()
            .with_field("entityId", entity_id)
            .with_entity_type(entity_type)
            .with_order_by("timestamp", true)
            .with_limit(1);
        Ok(self
            .entity_store
            .list(Some(&filter))
            .await?
            .into_iter()
            .next())
    }

    async fn delete_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<u64, StorageError> {
        let entries = self.list_by_entity(entity_id, entity_type).await?;
        let count = entries.len() as u64;
        for entry in &entries {
            self.entity_store.delete(&entry.id).await?;
        }
        Ok(count)
    }

    async fn get_entity_metadata(
        &self,
        entity_id: &str,
    ) -> Result<Option<HashMap<String, Value>>, StorageError> {
        match self.store().load(entity_id).await? {
            Some((_, meta)) => {
                let map = meta
                    .as_object()
                    .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                Ok(map)
            }
            None => Ok(None),
        }
    }

    async fn set_entity_metadata(
        &self,
        entity_id: &str,
        metadata: &HashMap<String, Value>,
    ) -> Result<(), StorageError> {
        if let Some((data, mut meta)) = self.store().load(entity_id).await? {
            if let Some(obj) = meta.as_object_mut() {
                for (k, v) in metadata {
                    obj.insert(k.clone(), v.clone());
                }
            }
            self.store().save(entity_id, &data, &meta).await?;
        }
        Ok(())
    }
}

// ─── TaskStorageAdapter ───

impl<S: Store> TaskStorageAdapter for TaskStorage<S> {
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        self.count_by_field("status").await
    }

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError> {
        let filter = QueryFilter::new().with_field_lt("createdAt", older_than);
        let all = self.entity_store.list(Some(&filter)).await?;
        let mut deleted = 0u64;
        for task in &all {
            self.entity_store.delete(&task.id).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

// ─── AgentExecutionStorageAdapter ───

impl<S: Store> AgentExecutionStorageAdapter for AgentExecutionStorage<S> {
    async fn list_by_definition(
        &self,
        definition_id: &str,
    ) -> Result<Vec<wf_types::AgentExecution>, StorageError> {
        let filter = QueryFilter::new().with_field("definitionId", definition_id);
        self.entity_store.list(Some(&filter)).await
    }

    async fn update_status(
        &self,
        id: &str,
        status: &wf_types::ExecutionStatus,
    ) -> Result<(), StorageError> {
        if let Some(mut entity) = self.entity_store.load(id).await? {
            entity.status = status.clone();
            self.entity_store.save(&entity).await?;
        }
        Ok(())
    }
}

// ─── AgentLoopStorageAdapter ───

impl<S: Store> AgentLoopStorageAdapter for AgentLoopStorage<S> {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        if let Some(mut entity) = self.entity_store.load(id).await? {
            entity.status = status.to_string();
            self.entity_store.save(&entity).await?;
        }
        Ok(())
    }

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("status", status);
        self.entity_store.list(Some(&filter)).await
    }

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        self.count_by_field("status").await
    }
}

// ─── TriggerStorageAdapter ───

impl<S: Store> TriggerStorageAdapter for TriggerStorage<S> {
    async fn list_by_event(
        &self,
        event: &str,
    ) -> Result<Vec<wf_types::TriggerStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("event", event);
        self.entity_store.list(Some(&filter)).await
    }

    async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<wf_types::TriggerStorageMetadata>, StorageError> {
        self.entity_store
            .mutate(id, |trigger| {
                trigger.enabled = enabled;
                trigger.updated_at = chrono::Utc::now().timestamp_millis();
                Ok(())
            })
            .await
    }
}

// ─── ToolStorageAdapter ───

impl<S: Store> ToolStorageAdapter for ToolStorage<S> {
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        self.count_by_field("toolType").await
    }

    async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<wf_types::ToolStorageMetadata>, StorageError> {
        self.entity_store
            .mutate(id, |tool| {
                tool.enabled = enabled;
                tool.updated_at = chrono::Utc::now().timestamp_millis();
                Ok(())
            })
            .await
    }
}

// ─── ToolDefinitionStorageAdapter ───

impl<S: Store> ToolDefinitionStorageAdapter for ToolDefinitionStorage<S> {
    async fn list_by_tool_type(
        &self,
        tool_type: &str,
    ) -> Result<Vec<wf_types::tool::Tool>, StorageError> {
        let filter = QueryFilter::new().with_field("toolType", tool_type);
        self.entity_store.list(Some(&filter)).await
    }
}

// ─── ScriptStorageAdapter ───

impl<S: Store> ScriptStorageAdapter for ScriptStorage<S> {
    async fn list_by_language(
        &self,
        language: &str,
    ) -> Result<Vec<wf_types::ScriptStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("language", language);
        self.entity_store.list(Some(&filter)).await
    }

    async fn set_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<wf_types::ScriptStorageMetadata>, StorageError> {
        self.entity_store
            .mutate(id, |script| {
                script.enabled = enabled;
                script.updated_at = chrono::Utc::now().timestamp_millis();
                Ok(())
            })
            .await
    }
}

// ─── NodeTemplateStorageAdapter ───

impl<S: Store> NodeTemplateStorageAdapter for NodeTemplateStorage<S> {
    async fn list_by_node_type(
        &self,
        node_type: &str,
    ) -> Result<Vec<wf_types::NodeTemplateStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("nodeType", node_type);
        self.entity_store.list(Some(&filter)).await
    }
}

// ─── AgentProfileStorageAdapter ───

impl<S: Store> AgentProfileStorageAdapter for AgentProfileStorage<S> {
    async fn get_first(
        &self,
    ) -> Result<Option<wf_types::AgentProfileStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_limit(1);
        Ok(self
            .entity_store
            .list(Some(&filter))
            .await?
            .into_iter()
            .next())
    }
}

// ─── UserInteractionStorageAdapter ───

impl<S: Store> UserInteractionStorageAdapter for UserInteractionStorage<S> {
    async fn list_by_execution(
        &self,
        execution_id: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("executionId", execution_id);
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::UserInteractionStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("status", status);
        self.entity_store.list(Some(&filter)).await
    }

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        self.count_by_field("status").await
    }
}

// ─── TriggerExecutionStorageAdapter ───

impl<S: Store> TriggerExecutionStorageAdapter for TriggerExecutionStorage<S> {
    async fn list_by_trigger(
        &self,
        trigger_name: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("triggerName", trigger_name);
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_execution(
        &self,
        execution_id: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("executionId", execution_id);
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_workflow(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::TriggerExecutionStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("workflowId", workflow_id);
        self.entity_store.list(Some(&filter)).await
    }

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        let all = self.entity_store.list(None).await?;
        let mut stats = HashMap::new();
        for entry in &all {
            let key = if entry.success {
                "success".to_string()
            } else {
                "failed".to_string()
            };
            *stats.entry(key).or_insert(0) += 1;
        }
        Ok(stats)
    }

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError> {
        let filter = QueryFilter::new().with_field_lt("triggeredAt", older_than);
        let all = self.entity_store.list(Some(&filter)).await?;
        let mut deleted = 0u64;
        for entry in &all {
            self.entity_store.delete(&entry.id).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

// ─── MessageStorageAdapter ───

impl<S: Store> MessageStorageAdapter for MessageStorage<S> {
    async fn list_by_execution(
        &self,
        execution_id: &str,
        options: Option<MessageListOptions>,
    ) -> Result<Vec<wf_types::MessageStorageMetadata>, StorageError> {
        let options = options.unwrap_or_default();
        let mut options = options;
        options.execution_id_filter = Some(execution_id.to_string());
        let filter: QueryFilter = options.into();
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_agent_loop(
        &self,
        agent_loop_id: &str,
        options: Option<MessageListOptions>,
    ) -> Result<Vec<wf_types::MessageStorageMetadata>, StorageError> {
        let mut options = options.unwrap_or_default();
        options.agent_loop_id_filter = Some(agent_loop_id.to_string());
        let filter: QueryFilter = options.into();
        self.entity_store.list(Some(&filter)).await
    }

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        self.count_by_field("role").await
    }
}

// ─── VariableStorageAdapter ───

impl<S: Store> VariableStorageAdapter for VariableStorage<S> {
    async fn get_by_scope(
        &self,
        execution_id: Option<&str>,
        scope: &str,
        name: &str,
    ) -> Result<Option<wf_types::VariableStorageMetadata>, StorageError> {
        let id = wf_types::VariableStorageMetadata::composite_id(execution_id, scope, name);
        self.entity_store.load(&id).await
    }

    async fn list_by_execution(
        &self,
        execution_id: &str,
        options: Option<VariableListOptions>,
    ) -> Result<Vec<wf_types::VariableStorageMetadata>, StorageError> {
        let mut options = options.unwrap_or_default();
        options.execution_id_filter = Some(execution_id.to_string());
        let filter: QueryFilter = options.into();
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_scope(
        &self,
        scope: &str,
        options: Option<VariableListOptions>,
    ) -> Result<Vec<wf_types::VariableStorageMetadata>, StorageError> {
        let mut options = options.unwrap_or_default();
        options.scope_filter = Some(scope.to_string());
        let filter: QueryFilter = options.into();
        self.entity_store.list(Some(&filter)).await
    }

    async fn delete_by_execution(&self, execution_id: &str) -> Result<u64, StorageError> {
        let filter = QueryFilter::new().with_field("executionId", execution_id);
        let all = self.entity_store.list(Some(&filter)).await?;
        let mut deleted = 0u64;
        for record in &all {
            self.entity_store.delete(&record.id).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}

// ─── TriggerTemplateStorageAdapter ───

impl<S: Store> TriggerTemplateStorageAdapter for TriggerTemplateStorage<S> {
    async fn list_by_trigger_type(
        &self,
        trigger_type: &str,
    ) -> Result<Vec<wf_types::TriggerTemplateStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("triggerType", trigger_type);
        self.entity_store.list(Some(&filter)).await
    }

    async fn list_by_category(
        &self,
        category: &str,
    ) -> Result<Vec<wf_types::TriggerTemplateStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("category", category);
        self.entity_store.list(Some(&filter)).await
    }
}

// ─── MetricsStorageAdapter (standalone, no BaseStorageAdapter) ───

#[derive(Clone)]
pub struct MetricsStorage<S> {
    entity_store: EntityStore<S, MetricRecord>,
}

impl<S: Store> MetricsStorage<S> {
    pub fn new(store: S) -> Self {
        Self {
            entity_store: EntityStore::new(store),
        }
    }

    pub fn inner(&self) -> &S {
        self.entity_store.inner()
    }
}

impl<S: Store + BatchStore> MetricsStorageAdapter for MetricsStorage<S> {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError> {
        let records: Vec<MetricRecord> = points
            .iter()
            .cloned()
            .map(MetricRecord::from_point)
            .collect();
        self.entity_store.save_batch(&records).await
    }

    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricsDataPoint>, StorageError> {
        let filter = QueryFilter::new()
            .with_field("metricName", name)
            .with_timestamp_range(start_time, end_time)
            .with_order_by("timestamp", false);
        let records = self.entity_store.list(Some(&filter)).await?;
        Ok(records.into_iter().map(|r| r.point).collect())
    }

    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError> {
        let filter = QueryFilter::new().with_field_lt("timestamp", older_than);
        let records = self.entity_store.list(Some(&filter)).await?;
        let mut deleted = 0u64;
        for record in &records {
            self.entity_store.delete(&record.id).await?;
            deleted += 1;
        }
        Ok(deleted)
    }
}
