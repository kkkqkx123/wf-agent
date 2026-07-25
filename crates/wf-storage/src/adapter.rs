// ============================================================================
// Storage Error
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Entity not found: {0}")]
    NotFound(String),
    #[error("Entity already exists: {0}")]
    AlreadyExists(String),
    #[error("Storage connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Write operation failed: {0}")]
    WriteFailed(String),
    #[error("Read operation failed: {0}")]
    ReadFailed(String),
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
}

impl From<serde_json::Error> for StorageError {
    fn from(e: serde_json::Error) -> Self {
        StorageError::SerializationError(e.to_string())
    }
}

// ============================================================================
// Base Storage Adapter Trait
// ============================================================================

#[async_trait]
pub trait BaseStorageAdapter<TMetadata, TListOptions>: Send + Sync {
    async fn initialize(&self) -> Result<(), StorageError>;
    async fn close(&self) -> Result<(), StorageError>;

    async fn save(&self, entity: &TMetadata) -> Result<(), StorageError>;
    async fn load(&self, id: &str) -> Result<Option<TMetadata>, StorageError>;
    async fn delete(&self, id: &str) -> Result<bool, StorageError>;
    async fn list(&self, options: Option<TListOptions>) -> Result<Vec<TMetadata>, StorageError>;
    async fn clear(&self) -> Result<(), StorageError>;

    async fn exists(&self, id: &str) -> Result<bool, StorageError> {
        Ok(self.load(id).await?.is_some())
    }
}

// ============================================================================
// Workflow Storage
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct WorkflowListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub name_filter: Option<String>,
    pub type_filter: Option<String>,
}

#[async_trait]
pub trait WorkflowStorageAdapter: BaseStorageAdapter<wf_types::WorkflowTemplate, WorkflowListOptions> {
    async fn update_metadata(
        &self,
        id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError>;

    async fn save_version(
        &self,
        workflow_id: &str,
        version: &str,
        template: &wf_types::WorkflowTemplate,
    ) -> Result<(), StorageError>;

    async fn list_versions(
        &self,
        workflow_id: &str,
    ) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError>;

    async fn load_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<Option<wf_types::WorkflowTemplate>, StorageError>;

    async fn delete_version(
        &self,
        workflow_id: &str,
        version: &str,
    ) -> Result<bool, StorageError>;
}

// ============================================================================
// Workflow Execution Storage
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct WorkflowExecutionListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub workflow_id_filter: Option<String>,
}

#[async_trait]
pub trait WorkflowExecutionStorageAdapter:
    BaseStorageAdapter<wf_types::WorkflowExecution, WorkflowExecutionListOptions>
{
    async fn update_status(
        &self,
        id: &str,
        status: &wf_types::ExecutionStatus,
    ) -> Result<(), StorageError>;
}

// ============================================================================
// Checkpoint Storage
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct CheckpointListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub entity_type_filter: Option<String>,
    pub entity_id_filter: Option<String>,
}

#[async_trait]
pub trait CheckpointStorageAdapter:
    BaseStorageAdapter<wf_types::Checkpoint, CheckpointListOptions>
{
    async fn list_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Vec<wf_types::Checkpoint>, StorageError>;

    async fn get_latest_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<Option<wf_types::Checkpoint>, StorageError>;

    async fn delete_by_entity(
        &self,
        entity_id: &str,
        entity_type: &str,
    ) -> Result<u64, StorageError>;

    async fn get_entity_metadata(
        &self,
        entity_id: &str,
    ) -> Result<Option<HashMap<String, serde_json::Value>>, StorageError>;

    async fn set_entity_metadata(
        &self,
        entity_id: &str,
        metadata: &HashMap<String, serde_json::Value>,
    ) -> Result<(), StorageError>;
}

// ============================================================================
// Task Storage
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct TaskListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
    pub task_type_filter: Option<String>,
}

#[async_trait]
pub trait TaskStorageAdapter:
    BaseStorageAdapter<wf_types::TaskStorageMetadata, TaskListOptions>
{
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError>;
}

// ============================================================================
// Agent Loop Storage
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct AgentLoopListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub status_filter: Option<String>,
}

#[async_trait]
pub trait AgentLoopStorageAdapter:
    BaseStorageAdapter<wf_types::AgentLoopStorageMetadata, AgentLoopListOptions>
{
    async fn update_status(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), StorageError>;

    async fn list_by_status(
        &self,
        status: &str,
    ) -> Result<Vec<wf_types::AgentLoopStorageMetadata>, StorageError>;

    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError>;
}

// ============================================================================
// Simple CRUD Storage Adapters (type aliases with default list options)
// ============================================================================

#[derive(Debug, Clone, Default)]
pub struct SimpleListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

#[async_trait]
pub trait AgentProfileStorageAdapter:
    BaseStorageAdapter<wf_types::AgentProfileStorageMetadata, SimpleListOptions>
{
}

#[async_trait]
pub trait ScriptStorageAdapter:
    BaseStorageAdapter<wf_types::ScriptStorageMetadata, SimpleListOptions>
{
}

#[async_trait]
pub trait ToolStorageAdapter:
    BaseStorageAdapter<wf_types::ToolStorageMetadata, SimpleListOptions>
{
}

#[async_trait]
pub trait HookTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::HookTemplateStorageMetadata, SimpleListOptions>
{
}

#[async_trait]
pub trait NodeTemplateStorageAdapter:
    BaseStorageAdapter<wf_types::NodeTemplateStorageMetadata, SimpleListOptions>
{
}

#[async_trait]
pub trait TriggerStorageAdapter:
    BaseStorageAdapter<wf_types::TriggerStorageMetadata, SimpleListOptions>
{
}

// ============================================================================
// Metrics Storage
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsDataPoint {
    pub name: String,
    pub value: f64,
    pub timestamp: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
}

#[async_trait]
pub trait MetricsStorageAdapter: Send + Sync {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError>;
    async fn query(
        &self,
        name: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricsDataPoint>, StorageError>;
    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError>;
}

// ============================================================================
// File Checkpoint Storage
// ============================================================================

#[async_trait]
pub trait FileCheckpointStorageAdapter:
    BaseStorageAdapter<wf_types::FileCheckpointStorageMetadata, SimpleListOptions>
{
    async fn load_by_file_path(
        &self,
        file_path: &str,
    ) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError>;
}
