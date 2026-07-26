use std::path::PathBuf;

use tracing::{info, warn};

use wf_storage::domain::store::Store;
use wf_storage::store::memory::MemoryStorage;

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
        workflow: MemoryStorage,
        execution: MemoryStorage,
        checkpoint: MemoryStorage,
    },
    #[cfg(feature = "sqlite")]
    SQLite {
        workflow: wf_storage::store::sqlite::SqliteStorage,
        execution: wf_storage::store::sqlite::SqliteStorage,
        checkpoint: wf_storage::store::sqlite::SqliteStorage,
    },
    #[cfg(feature = "postgres")]
    Postgres {
        workflow: wf_storage::store::postgres::PostgresStorage,
        execution: wf_storage::store::postgres::PostgresStorage,
        checkpoint: wf_storage::store::postgres::PostgresStorage,
    },
}

impl std::fmt::Debug for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageBackend::Memory { .. } => write!(f, "StorageBackend::Memory"),
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite { .. } => write!(f, "StorageBackend::SQLite"),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres { .. } => write!(f, "StorageBackend::Postgres"),
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

        let workflow = MemoryStorage::new("workflow");
        let execution = MemoryStorage::new("execution");
        let checkpoint = MemoryStorage::new("checkpoint");

        self.backend = Some(StorageBackend::Memory {
            workflow,
            execution,
            checkpoint,
        });

        info!("Memory storage initialized with all adapters");
        Ok(())
    }

    #[cfg(feature = "sqlite")]
    async fn initialize_sqlite(&mut self) -> RuntimeResult<()> {
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

        let workflow = SqliteStorage::new(&path_str, "workflow").await?;
        let execution = SqliteStorage::new(&path_str, "execution").await?;
        let checkpoint = SqliteStorage::new(&path_str, "checkpoint").await?;

        self.backend = Some(StorageBackend::SQLite {
            workflow,
            execution,
            checkpoint,
        });

        info!("SQLite storage initialized with all adapters");
        Ok(())
    }

    #[cfg(feature = "postgres")]
    async fn initialize_postgres(&mut self) -> RuntimeResult<()> {
        use wf_storage::store::postgres::PostgresStorage;

        let pg_config =
            self.config.postgres.as_ref().ok_or_else(|| {
                RuntimeError::Config("PostgreSQL storage config is missing".into())
            })?;

        info!("Initializing PostgreSQL storage");

        let workflow = PostgresStorage::new(&pg_config.connection_string, "workflow").await?;
        let execution = PostgresStorage::new(&pg_config.connection_string, "execution").await?;
        let checkpoint = PostgresStorage::new(&pg_config.connection_string, "checkpoint").await?;

        self.backend = Some(StorageBackend::Postgres {
            workflow,
            execution,
            checkpoint,
        });

        info!("PostgreSQL storage initialized with all adapters");
        Ok(())
    }

    pub fn workflow(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
            StorageBackend::Memory { workflow, .. } => Ok(workflow),
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite { workflow, .. } => Ok(workflow),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres { workflow, .. } => Ok(workflow),
        }
    }

    pub fn execution(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
            StorageBackend::Memory { execution, .. } => Ok(execution),
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite { execution, .. } => Ok(execution),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres { execution, .. } => Ok(execution),
        }
    }

    pub fn checkpoint(&self) -> RuntimeResult<&dyn wf_storage::domain::store::Store> {
        match self.backend.as_ref().ok_or(RuntimeError::NotInitialized)? {
            StorageBackend::Memory { checkpoint, .. } => Ok(checkpoint),
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite { checkpoint, .. } => Ok(checkpoint),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres { checkpoint, .. } => Ok(checkpoint),
        }
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
            } => {
                if let Err(e) = workflow.clear().await {
                    warn!("Error clearing workflow storage: {}", e);
                }
                if let Err(e) = execution.clear().await {
                    warn!("Error clearing execution storage: {}", e);
                }
                if let Err(e) = checkpoint.clear().await {
                    warn!("Error clearing checkpoint storage: {}", e);
                }
            }
            #[cfg(feature = "sqlite")]
            StorageBackend::SQLite {
                workflow,
                execution,
                checkpoint,
            } => {
                if let Err(e) = workflow.clear().await {
                    warn!("Error clearing workflow storage: {}", e);
                }
                if let Err(e) = execution.clear().await {
                    warn!("Error clearing execution storage: {}", e);
                }
                if let Err(e) = checkpoint.clear().await {
                    warn!("Error clearing checkpoint storage: {}", e);
                }
            }
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres {
                workflow,
                execution,
                checkpoint,
            } => {
                if let Err(e) = workflow.clear().await {
                    warn!("Error clearing workflow storage: {}", e);
                }
                if let Err(e) = execution.clear().await {
                    warn!("Error clearing execution storage: {}", e);
                }
                if let Err(e) = checkpoint.clear().await {
                    warn!("Error clearing checkpoint storage: {}", e);
                }
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
