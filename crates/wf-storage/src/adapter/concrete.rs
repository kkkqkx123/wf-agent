use std::collections::HashMap;
use serde_json::Value;

use crate::adapter::base::ListOptions;
use crate::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use crate::adapter::execution::{WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter};
use crate::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use crate::adapter::task::{TaskListOptions, TaskStorageAdapter};
use crate::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use crate::adapter::file_checkpoint::FileCheckpointStorageAdapter;
use crate::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use crate::domain::Store;
use crate::domain::QueryFilter;
use crate::error::StorageError;
use crate::make_base_adapter;
use crate::store::MemoryStorage;
#[cfg(feature = "sqlite")]
use crate::store::SqliteStorage;
#[cfg(feature = "postgres")]
use crate::store::PostgresStorage;

// ─── Macro invocation: generates BaseStorageAdapter impl + struct ───

make_base_adapter!(WorkflowStorage, wf_types::WorkflowDefinition, WorkflowListOptions);
make_base_adapter!(WorkflowExecutionStorage, wf_types::WorkflowExecution, WorkflowExecutionListOptions);
make_base_adapter!(CheckpointStorage, wf_types::Checkpoint, CheckpointListOptions);
make_base_adapter!(TaskStorage, wf_types::TaskStorageMetadata, TaskListOptions);
make_base_adapter!(AgentLoopStorage, wf_types::AgentLoopStorageMetadata, AgentLoopListOptions);
make_base_adapter!(FileCheckpointStorage, wf_types::FileCheckpointStorageMetadata, ListOptions);

// ─── Type aliases ───

pub type MemoryWorkflowStorage = WorkflowStorage<MemoryStorage>;
pub type MemoryWorkflowExecutionStorage = WorkflowExecutionStorage<MemoryStorage>;
pub type MemoryCheckpointStorage = CheckpointStorage<MemoryStorage>;
pub type MemoryTaskStorage = TaskStorage<MemoryStorage>;
pub type MemoryAgentLoopStorage = AgentLoopStorage<MemoryStorage>;
pub type MemoryFileCheckpointStorage = FileCheckpointStorage<MemoryStorage>;

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
pub type SqliteFileCheckpointStorage = FileCheckpointStorage<SqliteStorage>;

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
pub type PostgresFileCheckpointStorage = FileCheckpointStorage<PostgresStorage>;

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

    async fn delete_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<bool, StorageError> {
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
                let map = meta.as_object()
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

// ─── AgentLoopStorageAdapter ───

impl<S: Store> AgentLoopStorageAdapter for AgentLoopStorage<S> {
    async fn update_status(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StorageError> {
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
            .with_field("metricName", name);
        let entries = self.store.list(Some(&filter)).await?;
        let mut results = Vec::new();
        for (id, _) in entries {
            if let Some((data, _)) = self.store.load(&id).await? {
                if let Ok(point) = serde_json::from_slice::<MetricsDataPoint>(&data) {
                    if point.timestamp >= start_time && point.timestamp <= end_time {
                        results.push(point);
                    }
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
