use serde_json::Value;
use std::collections::HashMap;

use crate::adapter::agent_execution::{AgentExecutionListOptions, AgentExecutionStorageAdapter};
use crate::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use crate::adapter::agent_profile::{AgentProfileListOptions, AgentProfileStorageAdapter};
use crate::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use crate::adapter::execution::{WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter};
use crate::adapter::file_checkpoint::FileCheckpointListOptions;
use crate::adapter::file_checkpoint::FileCheckpointStorageAdapter;
use crate::adapter::hook_template::{HookTemplateListOptions, HookTemplateStorageAdapter};
use crate::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use crate::adapter::node_template::{NodeTemplateListOptions, NodeTemplateStorageAdapter};
use crate::adapter::script::{ScriptListOptions, ScriptStorageAdapter};
use crate::adapter::task::{TaskListOptions, TaskStorageAdapter};
use crate::adapter::tool::{ToolListOptions, ToolStorageAdapter};
use crate::adapter::trigger::{TriggerListOptions, TriggerStorageAdapter};
use crate::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use crate::domain::QueryFilter;
use crate::domain::Store;
use crate::error::StorageError;
use crate::make_base_adapter;
use crate::store::MemoryStorage;
#[cfg(feature = "postgres")]
use crate::store::PostgresStorage;
#[cfg(feature = "sqlite")]
use crate::store::SqliteStorage;

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
    FileCheckpointStorage,
    wf_types::FileCheckpointStorageMetadata,
    FileCheckpointListOptions
);
make_base_adapter!(
    TriggerStorage,
    wf_types::TriggerStorageMetadata,
    TriggerListOptions
);
make_base_adapter!(ToolStorage, wf_types::ToolStorageMetadata, ToolListOptions);
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
    HookTemplateStorage,
    wf_types::HookTemplateStorageMetadata,
    HookTemplateListOptions
);
make_base_adapter!(
    AgentProfileStorage,
    wf_types::AgentProfileStorageMetadata,
    AgentProfileListOptions
);

// ─── Type aliases ───

pub type MemoryWorkflowStorage = WorkflowStorage<MemoryStorage>;
pub type MemoryWorkflowExecutionStorage = WorkflowExecutionStorage<MemoryStorage>;
pub type MemoryCheckpointStorage = CheckpointStorage<MemoryStorage>;
pub type MemoryTaskStorage = TaskStorage<MemoryStorage>;
pub type MemoryAgentLoopStorage = AgentLoopStorage<MemoryStorage>;
pub type MemoryAgentExecutionStorage = AgentExecutionStorage<MemoryStorage>;
pub type MemoryFileCheckpointStorage = FileCheckpointStorage<MemoryStorage>;
pub type MemoryTriggerStorage = TriggerStorage<MemoryStorage>;
pub type MemoryToolStorage = ToolStorage<MemoryStorage>;
pub type MemoryScriptStorage = ScriptStorage<MemoryStorage>;
pub type MemoryNodeTemplateStorage = NodeTemplateStorage<MemoryStorage>;
pub type MemoryHookTemplateStorage = HookTemplateStorage<MemoryStorage>;
pub type MemoryAgentProfileStorage = AgentProfileStorage<MemoryStorage>;

#[cfg(feature = "sqlite")]
pub type SqliteWorkflowStorage = WorkflowStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteWorkflowExecutionStorage = WorkflowExecutionStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteCheckpointStorage = CheckpointStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteTaskStorage = TaskStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteAgentLoopStorage = AgentLoopStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteAgentExecutionStorage = AgentExecutionStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteFileCheckpointStorage = FileCheckpointStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteTriggerStorage = TriggerStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteToolStorage = ToolStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteScriptStorage = ScriptStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteNodeTemplateStorage = NodeTemplateStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteHookTemplateStorage = HookTemplateStorage<SqliteStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteAgentProfileStorage = AgentProfileStorage<SqliteStorage>;

#[cfg(feature = "postgres")]
pub type PostgresWorkflowStorage = WorkflowStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresWorkflowExecutionStorage = WorkflowExecutionStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresCheckpointStorage = CheckpointStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresTaskStorage = TaskStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresAgentLoopStorage = AgentLoopStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresAgentExecutionStorage = AgentExecutionStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresFileCheckpointStorage = FileCheckpointStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresTriggerStorage = TriggerStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresToolStorage = ToolStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresScriptStorage = ScriptStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresNodeTemplateStorage = NodeTemplateStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresHookTemplateStorage = HookTemplateStorage<PostgresStorage>;
#[cfg(feature = "postgres")]
pub type PostgresAgentProfileStorage = AgentProfileStorage<PostgresStorage>;

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
        let all = self.entity_store.list_metadata(None).await?;
        let mut results = Vec::new();
        for (id, _) in all {
            if id.starts_with(&prefix) {
                if let Some(entity) = self.entity_store.load(&id).await? {
                    results.push(entity);
                }
            }
        }
        Ok(results)
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
        self.entity_store.delete(&composite_id).await?;
        Ok(true)
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
        let mut results = self.list_by_entity(entity_id, entity_type).await?;
        results.sort_by_key(|c| c.timestamp);
        Ok(results.into_iter().last())
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
        let all = self.entity_store.list(None).await?;
        let mut stats = HashMap::new();
        for task in &all {
            *stats.entry(task.status.clone()).or_insert(0) += 1;
        }
        Ok(stats)
    }

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError> {
        let all = self.entity_store.list(None).await?;
        let mut deleted = 0u64;
        for task in &all {
            if task.created_at < older_than {
                self.entity_store.delete(&task.id).await?;
                deleted += 1;
            }
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
        let all = self.entity_store.list(None).await?;
        let mut stats = HashMap::new();
        for entry in &all {
            *stats.entry(entry.status.clone()).or_insert(0) += 1;
        }
        Ok(stats)
    }
}

// ─── FileCheckpointStorageAdapter ───

impl<S: Store> FileCheckpointStorageAdapter for FileCheckpointStorage<S> {
    async fn load_by_file_path(
        &self,
        file_path: &str,
    ) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError> {
        let all = self.entity_store.list(None).await?;
        Ok(all.into_iter().find(|e| e.file_path == file_path))
    }

    async fn list_by_entity(
        &self,
        entity_id: &str,
    ) -> Result<Vec<wf_types::FileCheckpointStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("entityId", entity_id);
        self.entity_store.list(Some(&filter)).await
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
}

// ─── ToolStorageAdapter ───

impl<S: Store> ToolStorageAdapter for ToolStorage<S> {
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        let all = self.entity_store.list(None).await?;
        let mut stats = HashMap::new();
        for entry in &all {
            *stats.entry(entry.tool_type.clone()).or_insert(0) += 1;
        }
        Ok(stats)
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

// ─── HookTemplateStorageAdapter ───

impl<S: Store> HookTemplateStorageAdapter for HookTemplateStorage<S> {
    async fn list_by_hook_type(
        &self,
        hook_type: &str,
    ) -> Result<Vec<wf_types::HookTemplateStorageMetadata>, StorageError> {
        let filter = QueryFilter::new().with_field("hookType", hook_type);
        self.entity_store.list(Some(&filter)).await
    }
}

// ─── AgentProfileStorageAdapter ───

impl<S: Store> AgentProfileStorageAdapter for AgentProfileStorage<S> {
    async fn get_first(
        &self,
    ) -> Result<Option<wf_types::AgentProfileStorageMetadata>, StorageError> {
        let all = self.entity_store.list(None).await?;
        Ok(all.into_iter().next())
    }
}

// ─── MetricsStorageAdapter (standalone, no BaseStorageAdapter) ───

pub struct MetricsStorage<S> {
    store: S,
}

impl<S: Store> MetricsStorage<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn inner(&self) -> &S {
        &self.store
    }
}

pub type MemoryMetricsStorage = MetricsStorage<MemoryStorage>;
#[cfg(feature = "sqlite")]
pub type SqliteMetricsStorage = MetricsStorage<SqliteStorage>;
#[cfg(feature = "postgres")]
pub type PostgresMetricsStorage = MetricsStorage<PostgresStorage>;

impl<S: Store> MetricsStorageAdapter for MetricsStorage<S> {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError> {
        for point in points {
            let id = format!("metric:{}:{}", point.name, point.timestamp);
            let data = serde_json::to_vec(point)?;
            let metadata = serde_json::json!({
                "entityType": "metric",
                "metricName": point.name,
                "timestamp": point.timestamp,
                "compressed": false,
            });
            self.store.save(&id, &data, &metadata).await?;
        }
        Ok(())
    }

    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricsDataPoint>, StorageError> {
        let filter = QueryFilter::new()
            .with_field("metricName", name)
            .with_timestamp_range(start_time, end_time);
        let entries = self.store.list(Some(&filter)).await?;
        let mut results = Vec::new();
        for (id, _) in entries {
            if let Some((data, _)) = self.store.load(&id).await? {
                if let Ok(point) = serde_json::from_slice::<MetricsDataPoint>(&data) {
                    results.push(point);
                }
            }
        }
        Ok(results)
    }

    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError> {
        let all = self.store.list(None).await?;
        let mut deleted = 0u64;
        for (id, meta) in all {
            let ts = meta.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
            if ts < older_than {
                self.store.delete(&id).await?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }
}
