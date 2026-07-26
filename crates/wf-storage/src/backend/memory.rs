use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use serde::{Serialize, de::DeserializeOwned};

use crate::error::StorageError;
use crate::adapter::base::{BaseStorageAdapter, ListOptions};
use crate::adapter::workflow::{WorkflowListOptions, WorkflowStorageAdapter};
use crate::adapter::execution::{WorkflowExecutionListOptions, WorkflowExecutionStorageAdapter};
use crate::adapter::checkpoint::{CheckpointListOptions, CheckpointStorageAdapter};
use crate::adapter::task::{TaskListOptions, TaskStorageAdapter};
use crate::adapter::agent_loop::{AgentLoopListOptions, AgentLoopStorageAdapter};
use crate::adapter::metrics::{MetricsDataPoint, MetricsStorageAdapter};
use crate::adapter::file_checkpoint::FileCheckpointStorageAdapter;
use crate::backend::EntityStore;

// ============================================================================
// Core: In-Memory Store
// ============================================================================

#[derive(Debug, Clone)]
struct InnerStore {
    data: HashMap<String, serde_json::Value>,
    metadata: HashMap<String, HashMap<String, serde_json::Value>>,
}

impl InnerStore {
    fn new() -> Self {
        Self { data: HashMap::new(), metadata: HashMap::new() }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<InnerStore>>,
    name: String,
}

impl MemoryStorage {
    pub fn new(name: &str) -> Self {
        Self { inner: Arc::new(RwLock::new(InnerStore::new())), name: name.to_string() }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new("default")
    }
}

impl BaseStorageAdapter<serde_json::Value, ListOptions> for MemoryStorage {
    async fn initialize(&self) -> Result<(), StorageError> {
        tracing::debug!("MemoryStorage '{}' initialized", self.name);
        Ok(())
    }

    async fn close(&self) -> Result<(), StorageError> {
        tracing::debug!("MemoryStorage '{}' closed", self.name);
        Ok(())
    }

    async fn save(&self, entity: &serde_json::Value) -> Result<(), StorageError> {
        let id = entity
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| StorageError::InvalidQuery("Entity missing 'id' field".into()))?
            .to_string();
        let mut store = self.inner.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        store.data.insert(id, entity.clone());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let store = self.inner.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(store.data.get(id).cloned())
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let mut store = self.inner.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(store.data.remove(id).is_some())
    }

    async fn list(&self, _options: Option<ListOptions>) -> Result<Vec<serde_json::Value>, StorageError> {
        let store = self.inner.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(store.data.values().cloned().collect())
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let mut store = self.inner.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        store.data.clear();
        store.metadata.clear();
        Ok(())
    }
}

// ============================================================================
// Helper: typed serde helpers for adapters with custom ListOptions
// ============================================================================

fn serialize<T: Serialize>(entity: &T) -> Result<serde_json::Value, StorageError> {
    serde_json::to_value(entity).map_err(Into::into)
}

fn deserialize<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, StorageError> {
    serde_json::from_value(value).map_err(Into::into)
}

// ============================================================================
// Workflow Storage
// ============================================================================

pub struct MemoryWorkflowStorage {
    inner: MemoryStorage,
    versions: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl MemoryWorkflowStorage {
    pub fn new() -> Self {
        Self { inner: MemoryStorage::new("workflow"), versions: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl Default for MemoryWorkflowStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseStorageAdapter<wf_types::WorkflowTemplate, WorkflowListOptions> for MemoryWorkflowStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }

    async fn save(&self, entity: &wf_types::WorkflowTemplate) -> Result<(), StorageError> {
        self.inner.save(&serialize(entity)?).await
    }

    async fn load(&self, id: &str) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> {
        self.inner.load(id).await?.map(deserialize).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }

    async fn list(&self, options: Option<WorkflowListOptions>) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> {
        let values = self.inner.list(None).await?;
        let mut templates: Vec<wf_types::WorkflowTemplate> = values.into_iter().map(deserialize).collect::<Result<Vec<_>, _>>()?;
        if let Some(opts) = options {
            if let Some(name) = opts.name_filter { templates.retain(|t| t.name.contains(&name)); }
            if let Some(typ) = opts.type_filter { templates.retain(|t| format!("{:?}", t.template_type) == typ); }
        }
        Ok(templates)
    }

    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl WorkflowStorageAdapter for MemoryWorkflowStorage {
    async fn update_metadata(&self, _id: &str, _metadata: &HashMap<String, serde_json::Value>) -> Result<(), StorageError> {
        Ok(())
    }

    async fn save_version(&self, workflow_id: &str, version: &str, template: &wf_types::WorkflowTemplate) -> Result<(), StorageError> {
        let value = serialize(template)?;
        let mut all_versions = self.versions.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        all_versions.entry(workflow_id.to_string()).or_default().insert(version.to_string(), value);
        Ok(())
    }

    async fn list_versions(&self, workflow_id: &str) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> {
        let all_versions = self.versions.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        let versions = all_versions.get(workflow_id).cloned().unwrap_or_default();
        versions.into_values().map(deserialize).collect()
    }

    async fn load_version(&self, workflow_id: &str, version: &str) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> {
        let all_versions = self.versions.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        all_versions.get(workflow_id).and_then(|v| v.get(version)).cloned().map(deserialize).transpose()
    }

    async fn delete_version(&self, workflow_id: &str, version: &str) -> Result<bool, StorageError> {
        let mut all_versions = self.versions.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(all_versions.get_mut(workflow_id).and_then(|v| v.remove(version)).is_some())
    }
}

// ============================================================================
// Checkpoint Storage
// ============================================================================

pub struct MemoryCheckpointStorage {
    inner: MemoryStorage,
    entity_metadata: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl MemoryCheckpointStorage {
    pub fn new() -> Self {
        Self { inner: MemoryStorage::new("checkpoint"), entity_metadata: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl Default for MemoryCheckpointStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseStorageAdapter<wf_types::Checkpoint, CheckpointListOptions> for MemoryCheckpointStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }

    async fn save(&self, entity: &wf_types::Checkpoint) -> Result<(), StorageError> {
        self.inner.save(&serialize(entity)?).await
    }

    async fn load(&self, id: &str) -> Result<Option<wf_types::Checkpoint>, StorageError> {
        self.inner.load(id).await?.map(deserialize).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }

    async fn list(&self, options: Option<CheckpointListOptions>) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let values = self.inner.list(None).await?;
        let mut checkpoints: Vec<wf_types::Checkpoint> = values.into_iter().map(deserialize).collect::<Result<Vec<_>, _>>()?;
        if let Some(opts) = options {
            if let Some(entity_id) = opts.entity_id_filter { checkpoints.retain(|c| c.entity_id == entity_id); }
            if let Some(entity_type) = opts.entity_type_filter { checkpoints.retain(|c| format!("{:?}", c.entity_type) == entity_type); }
        }
        Ok(checkpoints)
    }

    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl CheckpointStorageAdapter for MemoryCheckpointStorage {
    async fn list_by_entity(&self, entity_id: &str, _entity_type: &str) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let all = self.list(None).await?;
        Ok(all.into_iter().filter(|c| c.entity_id == entity_id).collect())
    }

    async fn get_latest_by_entity(&self, entity_id: &str, _entity_type: &str) -> Result<Option<wf_types::Checkpoint>, StorageError> {
        let all = self.list_by_entity(entity_id, "").await?;
        Ok(all.into_iter().max_by_key(|c| c.created_at))
    }

    async fn delete_by_entity(&self, entity_id: &str, _entity_type: &str) -> Result<u64, StorageError> {
        let all = self.list_by_entity(entity_id, "").await?;
        let count = all.len() as u64;
        for c in &all { let _ = self.inner.delete(&c.id).await; }
        Ok(count)
    }

    async fn get_entity_metadata(&self, entity_id: &str) -> Result<Option<HashMap<String, serde_json::Value>>, StorageError> {
        let meta = self.entity_metadata.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(meta.get(entity_id).cloned())
    }

    async fn set_entity_metadata(&self, entity_id: &str, metadata: &HashMap<String, serde_json::Value>) -> Result<(), StorageError> {
        let mut meta = self.entity_metadata.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        meta.insert(entity_id.to_string(), metadata.clone());
        Ok(())
    }
}

// ============================================================================
// Workflow Execution Storage
// ============================================================================

pub struct MemoryWorkflowExecutionStorage {
    inner: MemoryStorage,
}

impl MemoryWorkflowExecutionStorage {
    pub fn new() -> Self { Self { inner: MemoryStorage::new("workflow_execution") } }
}

impl Default for MemoryWorkflowExecutionStorage {
    fn default() -> Self { Self::new() }
}

impl BaseStorageAdapter<wf_types::WorkflowExecution, WorkflowExecutionListOptions> for MemoryWorkflowExecutionStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }

    async fn save(&self, entity: &wf_types::WorkflowExecution) -> Result<(), StorageError> {
        self.inner.save(&serialize(entity)?).await
    }

    async fn load(&self, id: &str) -> Result<Option<wf_types::WorkflowExecution>, StorageError> {
        self.inner.load(id).await?.map(deserialize).transpose()
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }

    async fn list(&self, _opts: Option<WorkflowExecutionListOptions>) -> Result<Vec<wf_types::WorkflowExecution>, StorageError> {
        let values = self.inner.list(None).await?;
        values.into_iter().map(deserialize).collect()
    }

    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl WorkflowExecutionStorageAdapter for MemoryWorkflowExecutionStorage {
    async fn update_status(&self, id: &str, status: &wf_types::ExecutionStatus) -> Result<(), StorageError> {
        let mut entity = self.load(id).await?.ok_or_else(|| StorageError::NotFound(format!("Execution {} not found", id)))?;
        entity.status = status.clone();
        self.save(&entity).await
    }
}

// ============================================================================
// Task Storage
// ============================================================================

pub struct MemoryTaskStorage {
    inner: MemoryStorage,
}

impl MemoryTaskStorage {
    pub fn new() -> Self { Self { inner: MemoryStorage::new("task") } }
}

impl Default for MemoryTaskStorage {
    fn default() -> Self { Self::new() }
}

impl BaseStorageAdapter<wf_types::TaskStorageMetadata, TaskListOptions> for MemoryTaskStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }
    async fn save(&self, entity: &wf_types::TaskStorageMetadata) -> Result<(), StorageError> { self.inner.save(&serialize(entity)?).await }
    async fn load(&self, id: &str) -> Result<Option<wf_types::TaskStorageMetadata>, StorageError> { self.inner.load(id).await?.map(deserialize).transpose() }
    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }
    async fn list(&self, _opts: Option<TaskListOptions>) -> Result<Vec<wf_types::TaskStorageMetadata>, StorageError> {
        self.inner.list(None).await?.into_iter().map(deserialize).collect()
    }
    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl TaskStorageAdapter for MemoryTaskStorage {
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        let all = self.list(None).await?;
        let mut stats = HashMap::new();
        for task in &all { *stats.entry(task.status.clone()).or_insert(0) += 1; }
        Ok(stats)
    }

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError> {
        let all = self.list(None).await?;
        let mut count = 0u64;
        for task in &all {
            if task.created_at < older_than { let _ = self.delete(&task.id).await; count += 1; }
        }
        Ok(count)
    }
}

// ============================================================================
// Agent Loop Storage
// ============================================================================

pub struct MemoryAgentLoopStorage {
    inner: MemoryStorage,
}

impl MemoryAgentLoopStorage {
    pub fn new() -> Self { Self { inner: MemoryStorage::new("agent_loop") } }
}

impl Default for MemoryAgentLoopStorage {
    fn default() -> Self { Self::new() }
}

impl BaseStorageAdapter<wf_types::AgentLoopStorageMetadata, AgentLoopListOptions> for MemoryAgentLoopStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }
    async fn save(&self, entity: &wf_types::AgentLoopStorageMetadata) -> Result<(), StorageError> { self.inner.save(&serialize(entity)?).await }
    async fn load(&self, id: &str) -> Result<Option<wf_types::AgentLoopStorageMetadata>, StorageError> { self.inner.load(id).await?.map(deserialize).transpose() }
    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }
    async fn list(&self, _opts: Option<AgentLoopListOptions>) -> Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError> {
        self.inner.list(None).await?.into_iter().map(deserialize).collect()
    }
    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl AgentLoopStorageAdapter for MemoryAgentLoopStorage {
    async fn update_status(&self, id: &str, status: &str) -> Result<(), StorageError> {
        let mut entity = self.load(id).await?.ok_or_else(|| StorageError::NotFound(format!("Agent loop {} not found", id)))?;
        entity.status = status.to_string();
        self.save(&entity).await
    }

    async fn list_by_status(&self, status: &str) -> Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError> {
        let all = self.list(None).await?;
        Ok(all.into_iter().filter(|a| a.status == status).collect())
    }

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        let all = self.list(None).await?;
        let mut stats = HashMap::new();
        for agent in &all { *stats.entry(agent.status.clone()).or_insert(0) += 1; }
        stats.entry("total_iterations".to_string()).or_insert(all.iter().map(|a| a.iteration_count as u64).sum());
        Ok(stats)
    }
}

// ============================================================================
// Metrics Storage (standalone — not based on MemoryStorage)
// ============================================================================

pub struct MemoryMetricsStorage {
    data: Arc<RwLock<Vec<MetricsDataPoint>>>,
}

impl MemoryMetricsStorage {
    pub fn new() -> Self { Self { data: Arc::new(RwLock::new(Vec::new())) } }
}

impl Default for MemoryMetricsStorage {
    fn default() -> Self { Self::new() }
}

impl MetricsStorageAdapter for MemoryMetricsStorage {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError> {
        let mut store = self.data.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        store.extend_from_slice(points);
        Ok(())
    }

    async fn query(&self, name: &str, start_time: i64, end_time: i64) -> Result<Vec<MetricsDataPoint>, StorageError> {
        let store = self.data.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(store.iter().filter(|p| p.name == name && p.timestamp >= start_time && p.timestamp <= end_time).cloned().collect())
    }

    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError> {
        let mut store = self.data.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        let before = store.len() as u64;
        store.retain(|p| p.timestamp >= older_than);
        Ok(before - store.len() as u64)
    }
}

// ============================================================================
// Simple CRUD Adapters (via EntityStore<MemoryStorage, T>)
// ============================================================================

pub type MemoryAgentProfileStorage = EntityStore<MemoryStorage, wf_types::AgentProfileStorageMetadata>;
pub type MemoryScriptStorage = EntityStore<MemoryStorage, wf_types::ScriptStorageMetadata>;
pub type MemoryToolStorage = EntityStore<MemoryStorage, wf_types::ToolStorageMetadata>;
pub type MemoryHookTemplateStorage = EntityStore<MemoryStorage, wf_types::HookTemplateStorageMetadata>;
pub type MemoryNodeTemplateStorage = EntityStore<MemoryStorage, wf_types::NodeTemplateStorageMetadata>;
pub type MemoryTriggerStorage = EntityStore<MemoryStorage, wf_types::TriggerStorageMetadata>;
pub type MemoryFileCheckpointStorage = EntityStore<MemoryStorage, wf_types::FileCheckpointStorageMetadata>;

impl FileCheckpointStorageAdapter for MemoryFileCheckpointStorage {
    async fn load_by_file_path(&self, file_path: &str) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError> {
        let all = self.list(None).await?;
        Ok(all.into_iter().find(|m| m.file_path == file_path))
    }
}
