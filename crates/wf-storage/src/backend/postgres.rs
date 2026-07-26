use std::collections::HashMap;
use serde::{Serialize, de::DeserializeOwned};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

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
// Core: PostgreSQL Store
// ============================================================================

pub struct PostgresStorage {
    pool: PgPool,
    table_name: String,
}

impl PostgresStorage {
    pub async fn new(connection_string: &str, table_name: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(connection_string)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        let create_sql = format!(
            "CREATE TABLE IF NOT EXISTS {} (id TEXT PRIMARY KEY, data JSONB NOT NULL, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL)",
            table_name
        );
        sqlx::query(&create_sql)
            .execute(&pool)
            .await
            .map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        Ok(Self { pool, table_name: table_name.to_string() })
    }
}

impl BaseStorageAdapter<serde_json::Value, ListOptions> for PostgresStorage {
    async fn initialize(&self) -> Result<(), StorageError> {
        tracing::debug!("PostgresStorage '{}' initialized", self.table_name);
        Ok(())
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.pool.close().await;
        Ok(())
    }

    async fn save(&self, entity: &serde_json::Value) -> Result<(), StorageError> {
        let id = entity.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| StorageError::InvalidQuery("Entity missing 'id' field".into()))?.to_string();
        let data = serde_json::to_string(entity)?;
        let now = chrono::Utc::now().timestamp_millis();
        let sql = format!(
            "INSERT INTO {} (id, data, created_at, updated_at) VALUES ($1, $2::jsonb, $3, $4) ON CONFLICT (id) DO UPDATE SET data = $2::jsonb, updated_at = $4",
            self.table_name
        );
        sqlx::query(&sql).bind(&id).bind(&data).bind(now).bind(now)
            .execute(&self.pool).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Option<serde_json::Value>, StorageError> {
        let sql = format!("SELECT data FROM {} WHERE id = $1", self.table_name);
        let result: Option<(serde_json::Value,)> = sqlx::query_as(&sql).bind(id)
            .fetch_optional(&self.pool).await.map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        Ok(result.map(|r| r.0))
    }

    async fn delete(&self, id: &str) -> Result<bool, StorageError> {
        let sql = format!("DELETE FROM {} WHERE id = $1", self.table_name);
        let result = sqlx::query(&sql).bind(id)
            .execute(&self.pool).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(result.rows_affected() > 0)
    }

    async fn list(&self, _options: Option<ListOptions>) -> Result<Vec<serde_json::Value>, StorageError> {
        let sql = format!("SELECT data FROM {}", self.table_name);
        let rows: Vec<(serde_json::Value,)> = sqlx::query_as(&sql)
            .fetch_all(&self.pool).await.map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    async fn clear(&self) -> Result<(), StorageError> {
        let sql = format!("DELETE FROM {}", self.table_name);
        sqlx::query(&sql).execute(&self.pool).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }
}

// ============================================================================
// Helper: typed serde for adapters with custom ListOptions
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

pub struct PostgresWorkflowStorage {
    inner: PostgresStorage,
}

impl PostgresWorkflowStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        Ok(Self { inner: PostgresStorage::new(connection_string, "workflow_templates").await? })
    }
}

impl BaseStorageAdapter<wf_types::WorkflowTemplate, WorkflowListOptions> for PostgresWorkflowStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }
    async fn save(&self, entity: &wf_types::WorkflowTemplate) -> Result<(), StorageError> { self.inner.save(&serialize(entity)?).await }
    async fn load(&self, id: &str) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> { self.inner.load(id).await?.map(deserialize).transpose() }
    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }
    async fn list(&self, _opts: Option<WorkflowListOptions>) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> {
        self.inner.list(None).await?.into_iter().map(deserialize).collect()
    }
    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl WorkflowStorageAdapter for PostgresWorkflowStorage {
    async fn update_metadata(&self, _id: &str, _metadata: &HashMap<String, serde_json::Value>) -> Result<(), StorageError> { Ok(()) }
    async fn save_version(&self, _workflow_id: &str, _version: &str, _template: &wf_types::WorkflowTemplate) -> Result<(), StorageError> { Ok(()) }
    async fn list_versions(&self, _workflow_id: &str) -> Result<Vec<wf_types::WorkflowTemplate>, StorageError> { Ok(Vec::new()) }
    async fn load_version(&self, _workflow_id: &str, _version: &str) -> Result<Option<wf_types::WorkflowTemplate>, StorageError> { Ok(None) }
    async fn delete_version(&self, _workflow_id: &str, _version: &str) -> Result<bool, StorageError> { Ok(false) }
}

// ============================================================================
// Workflow Execution Storage
// ============================================================================

pub struct PostgresWorkflowExecutionStorage {
    inner: PostgresStorage,
}

impl PostgresWorkflowExecutionStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        Ok(Self { inner: PostgresStorage::new(connection_string, "workflow_executions").await? })
    }
}

impl BaseStorageAdapter<wf_types::WorkflowExecution, WorkflowExecutionListOptions> for PostgresWorkflowExecutionStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }
    async fn save(&self, entity: &wf_types::WorkflowExecution) -> Result<(), StorageError> { self.inner.save(&serialize(entity)?).await }
    async fn load(&self, id: &str) -> Result<Option<wf_types::WorkflowExecution>, StorageError> { self.inner.load(id).await?.map(deserialize).transpose() }
    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }
    async fn list(&self, _opts: Option<WorkflowExecutionListOptions>) -> Result<Vec<wf_types::WorkflowExecution>, StorageError> {
        self.inner.list(None).await?.into_iter().map(deserialize).collect()
    }
    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl WorkflowExecutionStorageAdapter for PostgresWorkflowExecutionStorage {
    async fn update_status(&self, id: &str, status: &wf_types::ExecutionStatus) -> Result<(), StorageError> {
        let mut entity = self.load(id).await?.ok_or_else(|| StorageError::NotFound(format!("Execution {} not found", id)))?;
        entity.status = status.clone();
        self.save(&entity).await
    }
}

// ============================================================================
// Checkpoint Storage
// ============================================================================

pub struct PostgresCheckpointStorage {
    inner: PostgresStorage,
}

impl PostgresCheckpointStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        Ok(Self { inner: PostgresStorage::new(connection_string, "checkpoints").await? })
    }
}

impl BaseStorageAdapter<wf_types::Checkpoint, CheckpointListOptions> for PostgresCheckpointStorage {
    async fn initialize(&self) -> Result<(), StorageError> { self.inner.initialize().await }
    async fn close(&self) -> Result<(), StorageError> { self.inner.close().await }
    async fn save(&self, entity: &wf_types::Checkpoint) -> Result<(), StorageError> { self.inner.save(&serialize(entity)?).await }
    async fn load(&self, id: &str) -> Result<Option<wf_types::Checkpoint>, StorageError> { self.inner.load(id).await?.map(deserialize).transpose() }
    async fn delete(&self, id: &str) -> Result<bool, StorageError> { self.inner.delete(id).await }
    async fn list(&self, _opts: Option<CheckpointListOptions>) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        self.inner.list(None).await?.into_iter().map(deserialize).collect()
    }
    async fn clear(&self) -> Result<(), StorageError> { self.inner.clear().await }
}

impl CheckpointStorageAdapter for PostgresCheckpointStorage {
    async fn list_by_entity(&self, entity_id: &str, _entity_type: &str) -> Result<Vec<wf_types::Checkpoint>, StorageError> {
        self.list(None).await.map(|all| all.into_iter().filter(|c| c.entity_id == entity_id).collect())
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
    async fn get_entity_metadata(&self, _entity_id: &str) -> Result<Option<HashMap<String, serde_json::Value>>, StorageError> { Ok(None) }
    async fn set_entity_metadata(&self, _entity_id: &str, _metadata: &HashMap<String, serde_json::Value>) -> Result<(), StorageError> { Ok(()) }
}

// ============================================================================
// Task Storage
// ============================================================================

pub struct PostgresTaskStorage {
    inner: PostgresStorage,
}

impl PostgresTaskStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        Ok(Self { inner: PostgresStorage::new(connection_string, "tasks").await? })
    }
}

impl BaseStorageAdapter<wf_types::TaskStorageMetadata, TaskListOptions> for PostgresTaskStorage {
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

impl TaskStorageAdapter for PostgresTaskStorage {
    async fn get_stats(&self) -> Result<HashMap<String, u64>, StorageError> {
        let all = self.list(None).await?;
        let mut stats = HashMap::new();
        for task in &all { *stats.entry(task.status.clone()).or_insert(0) += 1; }
        Ok(stats)
    }

    async fn cleanup(&self, older_than: i64) -> Result<u64, StorageError> {
        let all = self.list(None).await?;
        let mut count = 0u64;
        for task in &all { if task.created_at < older_than { let _ = self.delete(&task.id).await; count += 1; } }
        Ok(count)
    }
}

// ============================================================================
// Agent Loop Storage
// ============================================================================

pub struct PostgresAgentLoopStorage {
    inner: PostgresStorage,
}

impl PostgresAgentLoopStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        Ok(Self { inner: PostgresStorage::new(connection_string, "agent_loops").await? })
    }
}

impl BaseStorageAdapter<wf_types::AgentLoopStorageMetadata, AgentLoopListOptions> for PostgresAgentLoopStorage {
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

impl AgentLoopStorageAdapter for PostgresAgentLoopStorage {
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
// Metrics Storage (dedicated table for time-series data)
// ============================================================================

pub struct PostgresMetricsStorage {
    pool: PgPool,
}

impl PostgresMetricsStorage {
    pub async fn new(connection_string: &str) -> Result<Self, StorageError> {
        let pool = PgPoolOptions::new().max_connections(5)
            .connect(connection_string).await.map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        sqlx::query("CREATE TABLE IF NOT EXISTS metrics (name TEXT NOT NULL, value DOUBLE PRECISION NOT NULL, timestamp BIGINT NOT NULL, tags JSONB)")
            .execute(&pool).await.map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_metrics_name_ts ON metrics(name, timestamp)")
            .execute(&pool).await.map_err(|e| StorageError::ConnectionFailed(e.to_string()))?;

        Ok(Self { pool })
    }
}

impl MetricsStorageAdapter for PostgresMetricsStorage {
    async fn save_batch(&self, points: &[MetricsDataPoint]) -> Result<(), StorageError> {
        let mut tx = self.pool.begin().await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        for point in points {
            let tags_json = point.tags.as_ref().map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null));
            sqlx::query("INSERT INTO metrics (name, value, timestamp, tags) VALUES ($1, $2, $3, $4)")
                .bind(&point.name).bind(point.value).bind(point.timestamp).bind(&tags_json)
                .execute(&mut *tx).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        }
        tx.commit().await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }

    async fn query(&self, name: &str, start_time: i64, end_time: i64) -> Result<Vec<MetricsDataPoint>, StorageError> {
        let rows: Vec<(String, f64, i64, Option<serde_json::Value>)> = sqlx::query_as(
            "SELECT name, value, timestamp, tags FROM metrics WHERE name = $1 AND timestamp >= $2 AND timestamp <= $3 ORDER BY timestamp"
        ).bind(name).bind(start_time).bind(end_time)
            .fetch_all(&self.pool).await.map_err(|e| StorageError::ReadFailed(e.to_string()))?;

        rows.into_iter().map(|(n, v, ts, tag_val)| {
            let tags = tag_val.and_then(|v| serde_json::from_value::<HashMap<String, String>>(v).ok());
            Ok(MetricsDataPoint { name: n, value: v, timestamp: ts, tags })
        }).collect()
    }

    async fn delete_old(&self, older_than: i64) -> Result<u64, StorageError> {
        let result = sqlx::query("DELETE FROM metrics WHERE timestamp < $1").bind(older_than)
            .execute(&self.pool).await.map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(result.rows_affected())
    }
}

// ============================================================================
// Simple CRUD Adapters (via EntityStore<PostgresStorage, T>)
// ============================================================================

pub type PostgresAgentProfileStorage = EntityStore<PostgresStorage, wf_types::AgentProfileStorageMetadata>;
pub type PostgresScriptStorage = EntityStore<PostgresStorage, wf_types::ScriptStorageMetadata>;
pub type PostgresToolStorage = EntityStore<PostgresStorage, wf_types::ToolStorageMetadata>;
pub type PostgresHookTemplateStorage = EntityStore<PostgresStorage, wf_types::HookTemplateStorageMetadata>;
pub type PostgresNodeTemplateStorage = EntityStore<PostgresStorage, wf_types::NodeTemplateStorageMetadata>;
pub type PostgresTriggerStorage = EntityStore<PostgresStorage, wf_types::TriggerStorageMetadata>;
pub type PostgresFileCheckpointStorage = EntityStore<PostgresStorage, wf_types::FileCheckpointStorageMetadata>;

impl FileCheckpointStorageAdapter for PostgresFileCheckpointStorage {
    async fn load_by_file_path(&self, file_path: &str) -> Result<Option<wf_types::FileCheckpointStorageMetadata>, StorageError> {
        let all = self.list(None).await?;
        Ok(all.into_iter().find(|m| m.file_path == file_path))
    }
}
