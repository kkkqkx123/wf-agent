use std::path::PathBuf;

use tracing::{info, warn};

use wf_storage::context::StorageContext;
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
    context: Option<StorageContext>,
}

impl std::fmt::Debug for StorageManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageManager")
            .field("config", &self.config)
            .field("initialized", &self.initialized)
            .finish()
    }
}

impl StorageManager {
    pub fn new(config: StorageConfig) -> Self {
        Self {
            config,
            initialized: false,
            context: None,
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn context(&self) -> RuntimeResult<&StorageContext> {
        self.context.as_ref().ok_or(RuntimeError::NotInitialized)
    }

    pub async fn initialize(&mut self) -> RuntimeResult<()> {
        if self.initialized {
            warn!("StorageManager already initialized");
            return Err(RuntimeError::AlreadyInitialized);
        }

        let ctx = match self.config.backend_type {
            StorageBackendType::Memory => {
                info!("Initializing in-memory storage");
                StorageContext::new_memory()
            }
            StorageBackendType::SQLite => {
                #[cfg(feature = "sqlite")]
                {
                    let app_name = self.config.app_name.as_deref().unwrap_or("app");
                    let db_path = self
                        .config
                        .sqlite
                        .as_ref()
                        .and_then(|c| c.db_path.clone())
                        .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)));
                    let path_str = db_path.to_string_lossy();
                    info!("Initializing SQLite storage at {:?}", db_path);
                    StorageContext::new_sqlite(&path_str).await?
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
                    let pg_config = self.config.postgres.as_ref().ok_or_else(|| {
                        RuntimeError::Config("PostgreSQL storage config is missing".into())
                    })?;
                    info!("Initializing PostgreSQL storage");
                    StorageContext::new_postgres(&pg_config.connection_string).await?
                }
                #[cfg(not(feature = "postgres"))]
                {
                    return Err(RuntimeError::Config(
                        "Postgres backend not available: enable the 'postgres' feature".into(),
                    ));
                }
            }
        };

        self.context = Some(ctx);
        self.initialized = true;
        info!("StorageManager initialized successfully");
        Ok(())
    }

    pub fn workflow(&self) -> RuntimeResult<&dyn Store> {
        Ok(self.context()?.workflow.store())
    }

    pub fn execution(&self) -> RuntimeResult<&dyn Store> {
        Ok(self.context()?.workflow_execution.store())
    }

    pub fn checkpoint(&self) -> RuntimeResult<&dyn Store> {
        Ok(self.context()?.checkpoint.store())
    }

    pub async fn close(&mut self) -> RuntimeResult<()> {
        if !self.initialized {
            return Ok(());
        }
        self.context = None;
        self.initialized = false;
        info!("StorageManager closed");
        Ok(())
    }

    pub async fn clear(&mut self) -> RuntimeResult<()> {
        let ctx = self.context.as_mut().ok_or(RuntimeError::NotInitialized)?;
        ctx.workflow.store().clear().await.ok();
        ctx.workflow_execution.store().clear().await.ok();
        ctx.checkpoint.store().clear().await.ok();
        ctx.task.store().clear().await.ok();
        ctx.agent_loop.store().clear().await.ok();
        ctx.metrics.inner().clear().await.ok();
        ctx.file_checkpoint.store().clear().await.ok();
        ctx.trigger.store().clear().await.ok();
        ctx.tool.store().clear().await.ok();
        ctx.script.store().clear().await.ok();
        ctx.node_template.store().clear().await.ok();
        ctx.hook_template.store().clear().await.ok();
        ctx.agent_profile.store().clear().await.ok();
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
