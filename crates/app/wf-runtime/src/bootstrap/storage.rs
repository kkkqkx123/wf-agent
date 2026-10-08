use std::path::PathBuf;
use std::sync::Arc;

use tracing::{info, warn};

use wf_types::config::storage::{StorageConfig, StorageType};

pub fn storage_db_path(config: &StorageConfig) -> PathBuf {
    let app_name = config.app_name.as_deref().unwrap_or("app");
    config
        .sqlite
        .as_ref()
        .map(|c| c.db_path.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("./storage/{}.db", app_name)))
}

/// Connection string for PostgreSQL backends. The structured config keeps a
/// full URL in `host` when the value comes from CLI parsing (`postgres://...`
/// or `postgres:<suffix>`); file-based configs fill the discrete fields
/// instead, in which case a URL is assembled here so every runtime switch
/// site shares one construction rule.
pub fn postgres_connection_string(
    config: &wf_types::config::storage::PostgresStorageConfig,
) -> String {
    if config.host.contains("://") {
        return config.host.clone();
    }
    if config.username.is_empty() && config.database.is_empty() {
        return config.host.clone();
    }
    let auth = if config.username.is_empty() {
        String::new()
    } else if config.password.is_empty() {
        format!("{}@", config.username)
    } else {
        format!("{}:{}@", config.username, config.password)
    };
    let db = if config.database.is_empty() {
        String::new()
    } else {
        format!("/{}", config.database)
    };
    format!("postgres://{}{}:{}{}", auth, config.host, config.port, db)
}

/// Event log persistence sharing the configured backend with entity and
/// checkpoint tables. The table is disjoint from both, so event writes never
/// contend with entity transactions; event loss never blocks execution.
/// Memory keeps events in the bounded bus window (`None`); Sqlite and
/// PostgreSQL each persist to their own backend with best-effort fallback.
pub async fn init_event_persistence(
    config: &StorageConfig,
) -> Option<Arc<dyn wf_api::PersistenceLayer>> {
    use wf_api::PersistenceLayer as ApiPersistenceLayer;

    match config.storage_type {
        StorageType::Memory => return None,
        StorageType::Sqlite => {}
        StorageType::Postgres => {}
    }
    if config.storage_type == StorageType::Postgres {
        let conn = config
            .postgres
            .as_ref()
            .map(postgres_connection_string)
            .unwrap_or_default();
        let layer = match wf_api::StorePersistenceLayer::postgres(&conn).await {
            Ok(store) => Arc::new(wf_api::BufferedPersistenceLayer::new(Arc::new(store))),
            Err(err) => {
                warn!(error = %err, "failed to open postgres event persistence backend; events stay in memory");
                return None;
            }
        };
        if let Err(err) = layer.initialize().await {
            warn!(error = %err, "failed to initialize event persistence backend; events stay in memory");
            return None;
        }
        info!("Event persistence enabled: postgres");
        return Some(layer as Arc<dyn ApiPersistenceLayer>);
    }
    let db_path = storage_db_path(config);

    let layer = match wf_api::StorePersistenceLayer::sqlite(&db_path.to_string_lossy()).await {
        Ok(store) => Arc::new(wf_api::BufferedPersistenceLayer::new(Arc::new(store))),
        Err(err) => {
            warn!(error = %err, path = %db_path.display(), "failed to open event persistence backend; events stay in memory");
            return None;
        }
    };
    if let Err(err) = layer.initialize().await {
        warn!(error = %err, "failed to initialize event persistence backend; events stay in memory");
        return None;
    }
    info!("Event persistence enabled: sqlite at {:?}", db_path);
    Some(layer as Arc<dyn ApiPersistenceLayer>)
}
