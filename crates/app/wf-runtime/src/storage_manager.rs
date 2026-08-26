use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use wf_storage::context::StorageContext;
use wf_storage::domain::Store;

use crate::error::{RuntimeError, RuntimeResult};

pub use wf_types::config::storage::StorageConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackendType {
    Memory,
    Sqlite,
    Postgres,
}

impl From<wf_types::config::storage::StorageType> for StorageBackendType {
    fn from(ty: wf_types::config::storage::StorageType) -> Self {
        match ty {
            wf_types::config::storage::StorageType::Sqlite => StorageBackendType::Sqlite,
            wf_types::config::storage::StorageType::Postgres => StorageBackendType::Postgres,
            wf_types::config::storage::StorageType::Memory => StorageBackendType::Memory,
        }
    }
}

pub struct StorageManager {
    config: StorageConfig,
    initialized: bool,
    context: Option<Arc<StorageContext>>,
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
        Ok(self.context.as_ref().ok_or(RuntimeError::NotInitialized)?)
    }

    /// Arc'd storage context for background tasks; `None` when uninitialized.
    pub fn shared_context(&self) -> Option<Arc<StorageContext>> {
        self.context.clone()
    }

    pub async fn initialize(&mut self) -> RuntimeResult<()> {
        if self.initialized {
            warn!("StorageManager already initialized");
            return Err(RuntimeError::AlreadyInitialized);
        }

        let backend: StorageBackendType = self.config.storage_type.clone().into();
        let ctx = match backend {
            StorageBackendType::Memory => {
                info!("Initializing in-memory storage");
                StorageContext::new_memory()
            }
            StorageBackendType::Sqlite => {
                #[cfg(feature = "sqlite")]
                {
                    let app_name = self.config.app_name.as_deref().unwrap_or("app");
                    let db_path = self
                        .config
                        .sqlite
                        .as_ref()
                        .map(|c| c.db_path.as_str())
                        .filter(|s| !s.is_empty())
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)));
                    let path_str = db_path.to_string_lossy();
                    info!("Initializing Sqlite storage at {:?}", db_path);
                    StorageContext::new_sqlite(&path_str).await?
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    return Err(RuntimeError::Config(
                        "Sqlite backend not available: enable the 'sqlite' feature".into(),
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
                    StorageContext::new_postgres(&pg_config.host).await?
                }
                #[cfg(not(feature = "postgres"))]
                {
                    return Err(RuntimeError::Config(
                        "Postgres backend not available: enable the 'postgres' feature".into(),
                    ));
                }
            }
        };

        self.context = Some(Arc::new(ctx));
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
        ctx.trigger.store().clear().await.ok();
        ctx.tool.store().clear().await.ok();
        ctx.script.store().clear().await.ok();
        ctx.node_template.store().clear().await.ok();
        ctx.agent_profile.store().clear().await.ok();
        ctx.trigger_template.store().clear().await.ok();
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

    fn memory_config() -> StorageConfig {
        StorageConfig {
            storage_type: wf_types::config::storage::StorageType::Memory,
            sqlite: None,
            postgres: None,
            app_name: None,
        }
    }

    #[tokio::test]
    async fn test_storage_manager_memory_init_close() {
        let mut manager = StorageManager::new(memory_config());

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
        let mut manager = StorageManager::new(memory_config());

        manager.initialize().await.unwrap();
        let result = manager.initialize().await;
        assert!(matches!(result, Err(RuntimeError::AlreadyInitialized)));

        manager.close().await.unwrap();
    }

    #[tokio::test]
    async fn test_storage_manager_not_initialized_accessors() {
        let manager = StorageManager::new(memory_config());

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
        let mut manager = StorageManager::new(memory_config());

        let result = manager.close().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_storage_manager_clear() {
        let mut manager = StorageManager::new(memory_config());

        manager.initialize().await.unwrap();
        manager.clear().await.unwrap();
        manager.close().await.unwrap();
    }

    #[test]
    fn test_storage_config_default() {
        let config = memory_config();
        assert_eq!(
            config.storage_type,
            wf_types::config::storage::StorageType::Memory
        );
        assert!(config.sqlite.is_none());
        assert!(config.postgres.is_none());
    }

    #[test]
    fn test_storage_backend_type_conversion() {
        let backend: StorageBackendType = wf_types::config::storage::StorageType::Sqlite.into();
        assert_eq!(backend, StorageBackendType::Sqlite);

        let backend: StorageBackendType = wf_types::config::storage::StorageType::Postgres.into();
        assert_eq!(backend, StorageBackendType::Postgres);

        let backend: StorageBackendType = wf_types::config::storage::StorageType::Memory.into();
        assert_eq!(backend, StorageBackendType::Memory);
    }
}
