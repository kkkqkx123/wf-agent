// ============================================================================
// In-Memory Store
// ============================================================================

#[derive(Debug, Clone)]
struct InnerStore {
    data: HashMap<String, serde_json::Value>,
    metadata: HashMap<String, HashMap<String, serde_json::Value>>,
}

impl InnerStore {
    fn new() -> Self {
        Self {
            data: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    inner: Arc<RwLock<InnerStore>>,
    name: String,
}

impl MemoryStorage {
    pub fn new(name: &str) -> Self {
        Self {
            inner: Arc::new(RwLock::new(InnerStore::new())),
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl BaseStorageAdapter<serde_json::Value, SimpleListOptions> for MemoryStorage {
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

    async fn list(
        &self,
        _options: Option<SimpleListOptions>,
    ) -> Result<Vec<serde_json::Value>, StorageError> {
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
// Memory Workflow Storage
// ============================================================================

pub struct MemoryWorkflowStorage {
    inner: MemoryStorage,
    versions: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl MemoryWorkflowStorage {
    pub fn new() -> Self {
        Self {
            inner: MemoryStorage::new("workflow"),
            versions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl BaseStorageAdapter<wf_types::WorkflowTemplate, WorkflowListOptions>
    for MemoryWorkflowStorage
{
    async fn initialize(&self) -> Result<(), StorageError> {
        self.inner.initialize().await
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.inner.close().await
    }

    async fn save(&self, entity: &wf_types::WorkflowTemplate) -> Result<(), StorageError> {
        let value = serde_json::to_value(entity)?;
        self.inner.save(&value).await
    }

    async fn load(&self, id: &str) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> {
        let value = self.inner.load(id).await?;
        value.map(|v| serde_json::from_value(v)).transpose().map_err(Into::into)
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        self.inner.delete(id).await
    }

    async fn list(
        &self,
        options: Option<WorkflowListOptions>,
    ) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> {
        let values = self.inner.list(None).await?;
        let mut templates: Vec<wf_types::WorkflowTemplate> = values
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(opts) = options {
            if let Some(name) = opts.name_filter {
                templates.retain(|t| t.name.contains(&name));
            }
            if let Some(typ) = opts.type_filter {
                templates.retain(|t| {
                    format!("{:?}", t.template_type) == typ
                });
            }
        }

        Ok(templates)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        self.inner.clear().await
    }
}

#[async_trait]
impl WorkflowStorageAdapter for MemoryWorkflowStorage {
    async fn update_metadata(
        &self,
        _id: &str,
        _metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn save_version(
        &self,
        workflow_id: &str,
        version: &str,
        template: &wf_types::WorkflowTemplate,
    ) -> Result<(), StorageError> {
        let value = serde_json::to_value(template)?;
        let mut all_versions = self.versions.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        all_versions
            .entry(workflow_id.to_string())
            .or_default()
            .insert(version.to_string(), value);
        Ok(())
    }

    async fn list_versions(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> {
        let all_versions = self.versions.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        let versions = all_versions.get(workflow_id).cloned().unwrap_or_default();
        versions
            .into_values()
            .map(|v| serde_json::from_value(v).map_err(Into::into))
            .collect()
    }

    async fn load_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> {
        let all_versions = self.versions.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        all_versions
            .get(workflow_id)
            .and_then(|v| v.get(version))
            .cloned()
            .map(|v| serde_json::from_value(v).map_err(Into::into))
            .transpose()
    }

    async fn delete_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<bool, StorageError> {
        let mut all_versions = self.versions.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(all_versions
            .get_mut(workflow_id)
            .and_then(|v| v.remove(version))
            .is_some())
    }
}

// ============================================================================
// Memory Checkpoint Storage
// ============================================================================

pub struct MemoryCheckpointStorage {
    inner: MemoryStorage,
    entity_metadata: Arc<RwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
}

impl MemoryCheckpointStorage {
    pub fn new() -> Self {
        Self {
            inner: MemoryStorage::new("checkpoint"),
            entity_metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl BaseStorageAdapter<wf_types::Checkpoint, CheckpointListOptions>
    for MemoryCheckpointStorage
{
    async fn initialize(&self) -> Result<(), StorageError> {
        self.inner.initialize().await
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.inner.close().await
    }

    async fn save(&self, entity: &wf_types::Checkpoint) -> Result<(), StorageError> {
        let value = serde_json::to_value(entity)?;
        self.inner.save(&value).await
    }

    async fn load(&self, id: &str) -> Result<Option<wf_types::Checkpoint>, StorageError> {
        let value = self.inner.load(id).await?;
        value.map(|v| serde_json::from_value(v)).transpose().map_err(Into::into)
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        self.inner.delete(id).await
    }

    async fn list(
        &self,
        options: Option<CheckpointListOptions>,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let values = self.inner.list(None).await?;
        let mut checkpoints: Vec<wf_types::Checkpoint> = values
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(opts) = options {
            if let Some(entity_id) = opts.entity_id_filter {
                checkpoints.retain(|c| c.entity_id == entity_id);
            }
            if let Some(entity_type) = opts.entity_type_filter {
                checkpoints.retain(|c| format!("{:?}", c.entity_type) == entity_type);
            }
        }

        Ok(checkpoints)
    }

    async fn clear(&self) -> Result<(), StorageError> {
        self.inner.clear().await
    }
}

#[async_trait]
impl CheckpointStorageAdapter for MemoryCheckpointStorage {
    async fn list_by_entity(
        &self,
        entity_id: &str,
        _entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        let all = self.list(None).await?;
        Ok(all.into_iter().filter(|c| c.entity_id == entity_id).collect())
    }

    async fn get_latest_by_entity(
        &self,
        entity_id: &str,
        _entity_type: &str,
    ) -> Result<Option<wf_types::Checkpoint>, StorageError> {
        let all = self.list_by_entity(entity_id, "").await?;
        Ok(all.into_iter().max_by_key(|c| c.created_at))
    }

    async fn delete_by_entity(
        &self,
        entity_id: &str,
        _entity_type: &str,
    ) -> Result<u64, StorageError> {
        let all = self.list_by_entity(entity_id, "").await?;
        let count = all.len() as u64;
        for checkpoint in &all {
            let _ = self.inner.delete(&checkpoint.id).await;
        }
        Ok(count)
    }

    async fn get_entity_metadata(
        &self,
        entity_id: &str,
    ) -> Result<Option<HashMap<String, serde_json::Value>>, StorageError> {
        let meta = self.entity_metadata.read().map_err(|e| StorageError::Internal(e.to_string()))?;
        Ok(meta.get(entity_id).cloned())
    }

    async fn set_entity_metadata(
        &self,
        entity_id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError> {
        let mut meta = self.entity_metadata.write().map_err(|e| StorageError::Internal(e.to_string()))?;
        meta.insert(entity_id.to_string(), metadata.clone());
        Ok(())
    }
}
