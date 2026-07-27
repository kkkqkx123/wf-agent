use std::path::PathBuf;

use tracing::{info, warn};

use wf_storage::adapter::concrete::{
    MemoryAgentLoopStorage, MemoryAgentProfileStorage, MemoryCheckpointStorage,
    MemoryFileCheckpointStorage, MemoryHookTemplateStorage, MemoryMetricsStorage,
    MemoryNodeTemplateStorage, MemoryScriptStorage, MemoryTaskStorage, MemoryToolStorage,
    MemoryTriggerStorage, MemoryWorkflowExecutionStorage, MemoryWorkflowStorage,
};
use wf_storage::domain::Store;

use crate::error::{RuntimeError, RuntimeResult};

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub backend_type: StorageBackendType,
    pub sqlite: Option<SqliteConfig>,
    pub postgres: Option<PostgresConfig>,
    pub app_name: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend_type: StorageBackendType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackendType {
    Memory,
    SQLite,
    Postgres,
}

#[derive(Debug, Clone)]
pub struct SqliteConfig {
    pub db_path: Option<PathBuf>,
    pub cache_size: Option<i32>,
    pub page_size: Option<i32>,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            cache_size: Some(64_000),
            page_size: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PostgresConfig {
    pub connection_string: String,
    pub max_connections: Option<u32>,
    pub min_connections: Option<u32>,
    pub idle_timeout: Option<u64>,
    pub connection_timeout: Option<u64>,
}

impl PostgresConfig {
    pub fn new(connection_string: impl Into<String>) -> Self {
        Self {
            connection_string: connection_string.into(),
            max_connections: None,
            min_connections: None,
            idle_timeout: None,
            connection_timeout: None,
        }
    }
}

pub struct StorageManager {
    config: StorageConfig,
    initialized: bool,
    backend: Option<StorageBackend>,
}

enum StorageBackend {
    Memory {
        workflow: MemoryWorkflowStorage,
        execution: MemoryWorkflowExecutionStorage,
        checkpoint: MemoryCheckpointStorage,
        task: MemoryTaskStorage,
        agent_loop: MemoryAgentLoopStorage,
        metrics: MemoryMetricsStorage,
        file_checkpoint: MemoryFileCheckpointStorage,
        trigger: MemoryTriggerStorage,
        tool: MemoryToolStorage,
        script: MemoryScriptStorage,
        node_template: MemoryNodeTemplateStorage,
        hook_template: MemoryHookTemplateStorage,
        agent_profile: MemoryAgentProfileStorage,
    },
    #[cfg(feature = "sqlite")]
    SQLite {
        workflow: wf_storage::adapter::concrete::SqliteWorkflowStorage,
        execution: wf_storage::adapter::concrete::SqliteWorkflowExecutionStorage,
        checkpoint: wf_storage::adapter::concrete::SqliteCheckpointStorage,
        task: wf_storage::adapter::concrete::SqliteTaskStorage,
        agent_loop: wf_storage::adapter::concrete::SqliteAgentLoopStorage,
        metrics: wf_storage::adapter::concrete::SqliteMetricsStorage,
        file_checkpoint: wf_storage::adapter::concrete::SqliteFileCheckpointStorage,
        trigger: wf_storage::adapter::concrete::SqliteTriggerStorage,
        tool: wf_storage::adapter::concrete::SqliteToolStorage,
        script: wf_storage::adapter::concrete::SqliteScriptStorage,
        node_template: wf_storage::adapter::concrete::SqliteNodeTemplateStorage,
        hook_template: wf_storage::adapter::concrete::SqliteHookTemplateStorage,
        agent_profile: wf_storage::adapter::concrete::SqliteAgentProfileStorage,
    },
    #[cfg(feature = "postgres")]
    Postgres {
        workflow: wf_storage::adapter::concrete::PostgresWorkflowStorage,
        execution: wf_storage::adapter::concrete::PostgresWorkflowExecutionStorage,
        checkpoint: wf_storage::adapter::concrete::PostgresCheckpointStorage,
        task: wf_storage::adapter::concrete::PostgresTaskStorage,
        agent_loop: wf_storage::adapter::concrete::PostgresAgentLoopStorage,
        metrics: wf_storage::adapter::concrete::PostgresMetricsStorage,
        file_checkpoint: wf_storage::adapter::concrete::PostgresFileCheckpointStorage,
        trigger: wf_storage::adapter::concrete::PostgresTriggerStorage,
        tool: wf_storage::adapter::concrete::PostgresToolStorage,
        script: wf_storage::adapter::concrete::PostgresScriptStorage,
        node_template: wf_storage::adapter::concrete::PostgresNodeTemplateStorage,
        hook_template: wf_storage::adapter::concrete::PostgresHookTemplateStorage,
        agent_profile: wf_storage::adapter::concrete::PostgresAgentProfileStorage,
    },
}

impl std::fmt::Debug for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory { .. } => write!(f, "StorageBackend::Memory"),
            #[cfg(feature = "sqlite")]
            Self::SQLite { .. } => write!(f, "StorageBackend::SQLite"),
            #[cfg(feature = "postgres")]
            Self::Postgres { .. } => write!(f, "StorageBackend::Postgres"),
        }
    }
}

impl StorageManager {
    pub fn new(config: StorageConfig) -> Self {
        Self {
            config,
            initialized: false,
            backend: None,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub async fn initialize(&mut self) -> RuntimeResult<()> {
        if self.initialized {
            warn!("StorageManager already initialized");
            return Err(RuntimeError::AlreadyInitialized);
        }

        match self.config.backend_type {
            StorageBackendType::Memory => self.initialize_memory().await?,
            StorageBackendType::SQLite => {
                #[cfg(feature = "sqlite")]
                {
                    self.initialize_sqlite().await?
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(RuntimeError::Config(
                        "SQLite backend not available: enable the 'sqlite' feature".into(),
                    ));
                }
            }
            StorageBackendType::Postgres => {
                #[cfg(feature = "postgres")]
                {
                    self.initialize_postgres().await?
                }
                #[cfg(not(feature = "postgres"))]
                {
                    return Err(RuntimeError::Config(
                        "Postgres backend not available: enable the 'postgres' feature".into(),
                    ));
                }
            }
        }

        self.initialized = true;
        info!("StorageManager initialized successfully");
        Ok(())
    }

    async fn initialize_memory(&mut self) -> RuntimeResult<()> {
        info!("Initializing in-memory storage");

        let workflow =
            MemoryWorkflowStorage::new(wf_storage::store::memory::MemoryStorage::new("workflow"));
        let execution = MemoryWorkflowExecutionStorage::new(
            wf_storage::store::memory::MemoryStorage::new("execution"),
        );
        let checkpoint = MemoryCheckpointStorage::new(
            wf_storage::store::memory::MemoryStorage::new("checkpoint"),
        );
        let task = MemoryTaskStorage::new(wf_storage::store::memory::MemoryStorage::new("task"));
        let agent_loop = MemoryAgentLoopStorage::new(
            wf_storage::store::memory::MemoryStorage::new("agent_loop"),
        );
        let metrics =
            MemoryMetricsStorage::new(wf_storage::store::memory::MemoryStorage::new("metrics"));
        let file_checkpoint = MemoryFileCheckpointStorage::new(
            wf_storage::store::memory::MemoryStorage::new("file_checkpoint"),
        );
        let trigger =
            MemoryTriggerStorage::new(wf_storage::store::memory::MemoryStorage::new("trigger"));
        let tool = MemoryToolStorage::new(wf_storage::store::memory::MemoryStorage::new("tool"));
        let script =
            MemoryScriptStorage::new(wf_storage::store::memory::MemoryStorage::new("script"));
        let node_template = MemoryNodeTemplateStorage::new(
            wf_storage::store::memory::MemoryStorage::new("node_template"),
        );
        let hook_template = MemoryHookTemplateStorage::new(
            wf_storage::store::memory::MemoryStorage::new("hook_template"),
        );
        let agent_profile = MemoryAgentProfileStorage::new(
            wf_storage::store::memory::MemoryStorage::new("agent_profile"),
        );

        self.backend = Some(StorageBackend::Memory {
            workflow,
            execution,
            checkpoint,
            task,
            agent_loop,
            metrics,
            file_checkpoint,
            trigger,
            tool,
            script,
            node_template,
            hook_template,
            agent_profile,
        });

        info!("Memory storage initialized with all adapters");
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    async fn initialize_sqlite(&mut self) -> RuntimeResult<()> {
        use wf_storage::adapter::concrete::{
            SqliteAgentLoopStorage, SqliteAgentProfileStorage, SqliteCheckpointStorage,
            SqliteFileCheckpointStorage, SqliteHookTemplateStorage, SqliteMetricsStorage,
            SqliteNodeTemplateStorage, SqliteScriptStorage, SqliteTaskStorage, SqliteToolStorage,
            SqliteTriggerStorage, SqliteWorkflowExecutionStorage, SqliteWorkflowStorage,
        };
        use wf_storage::store::sqlite::SqliteStorage;

        let app_name = self.config.app_name.as_deref().unwrap_or("app");
        let db_path = self
            .config
            .sqlite
            .as_ref()
            .and_then(|c| c.db_path.clone())
            .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)));

        let path_str = db_path.to_string_lossy();
        info!("Initializing SQLite storage at {:?}", db_path);

        let workflow = SqliteWorkflowStorage::new(SqliteStorage::new(&path_str, "workflow").await?);
        let execution =
            SqliteWorkflowExecutionStorage::new(SqliteStorage::new(&path_str, "execution").await?);
        let checkpoint =
            SqliteCheckpointStorage::new(SqliteStorage::new(&path_str, "checkpoint").await?);
        let task = SqliteTaskStorage::new(SqliteStorage::new(&path_str, "task").await?);
        let agent_loop =
            SqliteAgentLoopStorage::new(SqliteStorage::new(&path_str, "agent_loop").await?);
        let metrics = SqliteMetricsStorage::new(SqliteStorage::new(&path_str, "metrics").await?);
        let file_checkpoint = SqliteFileCheckpointStorage::new(
            SqliteStorage::new(&path_str, "file_checkpoint").await?,
        );
        let trigger = SqliteTriggerStorage::new(SqliteStorage::new(&path_str, "trigger").await?);
        let tool = SqliteToolStorage::new(SqliteStorage::new(&path_str, "tool").await?);
        let script = SqliteScriptStorage::new(SqliteStorage::new(&path_str, "script").await?);
        let node_template =
            SqliteNodeTemplateStorage::new(SqliteStorage::new(&path_str, "node_template").await?);
        let hook_template =
            SqliteHookTemplateStorage::new(SqliteStorage::new(&path_str, "hook_template").await?);
        let agent_profile =
            SqliteAgentProfileStorage::new(SqliteStorage::new(&path_str, "agent_profile").await?);

        self.backend = Some(StorageBackend::SQLite {
            workflow,
            execution,
            checkpoint,
            task,
            agent_loop,
            metrics,
            file_checkpoint,
            trigger,
            tool,
            script,
            node_template,
            hook_template,
            agent_profile,
        });

        info!("SQLite storage initialized with all adapters");
        Ok(())
    }

    #[cfg(feature = "postgres")]
    async fn initialize_postgres(&mut self) -> RuntimeResult<()> {
        use wf_storage::adapter::concrete::{
            PostgresAgentLoopStorage, PostgresAgentProfileStorage, PostgresCheckpointStorage,
            PostgresFileCheckpointStorage, PostgresHookTemplateStorage, PostgresMetricsStorage,
            PostgresNodeTemplateStorage, PostgresScriptStorage, PostgresTaskStorage,
            PostgresToolStorage, PostgresTriggerStorage, PostgresWorkflowExecutionStorage,
            PostgresWorkflowStorage,
        };
        use wf_storage::store::postgres::PostgresStorage;

        let pg_config =
            self.config.postgres.as_ref().ok_or_else(|| {
                RuntimeError::Config("PostgreSQL storage config is missing".into())
            })?;

        info!("Initializing PostgreSQL storage");

        let workflow = PostgresWorkflowStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "workflow").await?,
        );
        let execution = PostgresWorkflowExecutionStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "execution").await?,
        );
        let checkpoint = PostgresCheckpointStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "checkpoint").await?,
        );
        let task = PostgresTaskStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "task").await?,
        );
        let agent_loop = PostgresAgentLoopStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "agent_loop").await?,
        );
        let metrics = PostgresMetricsStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "metrics").await?,
        );
        let file_checkpoint = PostgresFileCheckpointStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "file_checkpoint").await?,
        );
        let trigger = PostgresTriggerStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "trigger").await?,
        );
        let tool = PostgresToolStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "tool").await?,
        );
        let script = PostgresScriptStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "script").await?,
        );
        let node_template = PostgresNodeTemplateStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "node_template").await?,
        );
        let hook_template = PostgresHookTemplateStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "hook_template").await?,
        );
        let agent_profile = PostgresAgentProfileStorage::new(
            PostgresStorage::new(&pg_config.connection_string, "agent_profile").await?,
        );

        self.backend = Some(StorageBackend::Postgres {
            workflow,
            execution,
            checkpoint,
            task,
            agent_loop,
            metrics,
            file_checkpoint,
            trigger,
            tool,
            script,
            node_template,
            hook_template,
            agent_profile,
        });

        info!("PostgreSQL storage initialized with all adapters");
        Ok(())
    }

    pub fn workflow(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        Ok(
            match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
                StorageBackend::Memory { workflow, .. } => workflow.store(),
                #[cfg(feature = "sqlite")]
                StorageBackend::SQLite { workflow, .. } => workflow.store(),
                #[cfg(feature = "postgres")]
                StorageBackend::Postgres { workflow, .. } => workflow.store(),
            },
        )
    }

    pub fn execution(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        Ok(
            match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
                StorageBackend::Memory { execution, .. } => execution.store(),
                #[cfg(feature = "sqlite")]
                StorageBackend::SQLite { execution, .. } => execution.store(),
                #[cfg(feature = "postgres")]
                StorageBackend::Postgres { execution, .. } => execution.store(),
            },
        )
    }

    pub fn checkpoint(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        Ok(
            match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
                StorageBackend::Memory { checkpoint, .. } => checkpoint.store(),
                #[cfg(feature = "sqlite")]
                StorageBackend::SQLite { checkpoint, .. } => checkpoint.store(),
                #[cfg(feature = "postgres")]
                StorageBackend::Postgres { checkpoint, .. } => checkpoint.store(),
            },
        )
    }

    pub async fn close(&mut self) -> RuntimeResult<()> {
        if !self.initialized {
            return Ok(());
        }

        self.backend = None;
        self.initialized = false;
        info!("StorageManager closed");
        Ok(())
    }

    pub async fn clear(&mut self) -> RuntimeResult<()> {
        if !self.initialized {
            return Ok(());
        }

        let backend = self.backend.as_mut().ok_or(RuntimeError::NotInitialized)?;

        match backend {
            StorageBackend::Memory {
                workflow,
                execution,
                checkpoint,
                task,
                agent_loop,
                metrics,
                file_checkpoint,
                trigger,
                tool,
                script,
                node_template,
                hook_template,
                agent_profile,
            } => {
                workflow.store().clear().await.ok();
                execution.store().clear().await.ok();
                checkpoint.store().clear().await.ok();
                task.store().clear().await.ok();
                agent_loop.store().clear().await.ok();
                metrics.inner().clear().await.ok();
                file_checkpoint.store().clear().await.ok();
                trigger.store().clear().await.ok();
                tool.store().clear().await.ok();
                script.store().clear().await.ok();
                node_template.store().clear().await.ok();
                hook_template.store().clear().await.ok();
                agent_profile.store().clear().await.ok();
            }
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite {
                workflow,
                execution,
                checkpoint,
                task,
                agent_loop,
                metrics,
                file_checkpoint,
                trigger,
                tool,
                script,
                node_template,
                hook_template,
                agent_profile,
            } => {
                workflow.store().clear().await.ok();
                execution.store().clear().await.ok();
                checkpoint.store().clear().await.ok();
                task.store().clear().await.ok();
                agent_loop.store().clear().await.ok();
                metrics.inner().clear().await.ok();
                file_checkpoint.store().clear().await.ok();
                trigger.store().clear().await.ok();
                tool.store().clear().await.ok();
                script.store().clear().await.ok();
                node_template.store().clear().await.ok();
                hook_template.store().clear().await.ok();
                agent_profile.store().clear().await.ok();
            }
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres {
                workflow,
                execution,
                checkpoint,
                task,
                agent_loop,
                metrics,
                file_checkpoint,
                trigger,
                tool,
                script,
                node_template,
                hook_template,
                agent_profile,
            } => {
                workflow.store().clear().await.ok();
                execution.store().clear().await.ok();
                checkpoint.store().clear().await.ok();
                task.store().clear().await.ok();
                agent_loop.store().clear().await.ok();
                metrics.inner().clear().await.ok();
                file_checkpoint.store().clear().await.ok();
                trigger.store().clear().await.ok();
                tool.store().clear().await.ok();
                script.store().clear().await.ok();
                node_template.store().clear().await.ok();
                hook_template.store().clear().await.ok();
                agent_profile.store().clear().await.ok();
            }
        }

        info!("StorageManager cleared");
        Ok(())
    }
}

impl Drop for StorageManager {
    fn drop(&mut self) {
        if self.initialized {
            warn!("StorageManager dropped without explicit close — data may not be flushed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_storage_manager_memory_init_close() {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        };
        let mut manager = StorageManager::new(config);

        assert!(!manager.is_initialized());
        manager.initialize().await.unwrap();
        assert!(manager.is_initialized());

        manager.workflow().unwrap();
        manager.execution().unwrap();
        manager.checkpoint().unwrap();

        manager.close().await.unwrap();
        assert!(!manager.is_initialized());
    }

    #[tokio::test]
    async fn test_storage_manager_double_init_fails() {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        };
        let mut manager = StorageManager::new(config);

        manager.initialize().await.unwrap();
        let result = manager.initialize().await;
        assert!(matches!(result, Err(RuntimeError::AlreadyInitialized)));

        manager.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_storage_manager_not_initialized_accessors() {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        };
        let manager = StorageManager::new(config);

        assert!(matches!(
            manager.workflow(),
            Err(RuntimeError::NotInitialized)
        ));
        assert!(matches!(
            manager.execution(),
            Err(RuntimeError::NotInitialized)
        ));
        assert!(matches!(
            manager.checkpoint(),
            Err(RuntimeError::NotInitialized)
        ));
    }

    #[tokio::test]
    async fn test_storage_manager_close_not_initialized() {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        };
        let mut manager = StorageManager::new(config);

        let result = manager.close().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_storage_manager_clear() {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        };
        let mut manager = StorageManager::new(config);

        manager.initialize().await.unwrap();
        manager.clear().await.unwrap();
        manager.close().await.unwrap();
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.backend_type, StorageBackendType::Memory);
        assert!(config.sqlite.is_none());
        assert!(config.postgres.is_none());
    }

    #[test]
    fn test_postgres_config_new() {
        let config = PostgresConfig::new("postgresql://localhost/test");
        assert_eq!(config.connection_string, "postgresql://localhost/test");
        assert!(config.max_connections.is_none());
    }
}
