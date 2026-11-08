use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use wf_types::config::storage::{StorageConfig, StorageType};

/// Environment variable overriding the storage data root.
pub const STORAGE_DATA_DIR_ENV: &str = "WF_STORAGE_DATA_DIR";

/// Top-level data dir for sqlite storage (created on demand). Per-user
/// so different OS users never share a database file.
pub fn default_storage_data_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var(STORAGE_DATA_DIR_ENV) {
        if !override_dir.trim().is_empty() {
            return PathBuf::from(override_dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        PathBuf::from(home).join("Library/Application Support")
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(home).join("AppData/Roaming"))
    } else {
        PathBuf::from(home).join(".local/share")
    }
}

/// Default sqlite database path under the per-user data dir.
pub fn default_storage_db_path(app_name: &str) -> PathBuf {
    default_storage_data_dir()
        .join("wf")
        .join("storage")
        .join(format!("{app_name}.db"))
}

pub fn storage_db_path(config: &StorageConfig) -> PathBuf {
    let app_name = config.app_name.as_deref().unwrap_or("app");
    config
        .sqlite
        .as_ref()
        .map(|c| c.db_path.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| default_storage_db_path(app_name))
}

/// Memory-like sqlite paths never touch the filesystem and need no lock.
pub fn is_memory_db_path(path: &Path) -> bool {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    trimmed == ":memory:" || trimmed == "file::memory:" || trimmed == "sqlite::memory:"
}

/// Absolute form of a database path for lock identity. Relative paths
/// resolve against the process working directory so the same spelling in
/// the same directory maps to the same lock.
pub fn absolutize_db_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    std::env::current_dir()
        .map(|cwd| cwd.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Sibling lock file guarding one sqlite database: `<db>.lock`.
pub fn storage_lock_path(db_path: &Path) -> PathBuf {
    let absolute = absolutize_db_path(db_path);
    PathBuf::from(format!("{}.lock", absolute.display()))
}

/// Acquire the single-instance guard for one sqlite database. The returned
/// file must stay open for the process lifetime; closing it releases the
/// guard. A second process guarding the same database fails instead of
/// opening the same file twice.
pub fn acquire_storage_lock(db_path: &Path) -> Result<std::fs::File, String> {
    let absolute = absolutize_db_path(db_path);
    if let Some(parent) = absolute.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create storage dir {}: {e}", parent.display()))?;
        }
    }
    let lock_path = storage_lock_path(&absolute);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(|e| format!("cannot open storage lock {}: {e}", lock_path.display()))?;
    // Cross-platform non-blocking exclusive guard (flock on Unix, LockFileEx
    // on Windows). The open file stays alive in the caller, so a crash
    // releases the guard without leaving stale state behind.
    {
        use fs4::fs_std::FileExt;
        let acquired = file
            .try_lock_exclusive()
            .map_err(|e| format!("cannot guard storage lock {}: {e}", lock_path.display()))?;
        if !acquired {
            return Err(format!(
                "sqlite storage is already in use (lock {}). Stop the running server, use --storage with an independent path, run as another user, or switch shared deployments to postgres.",
                lock_path.display()
            ));
        }
    }
    let _ = file.set_len(0);
    {
        use std::io::Write;
        let mut file_ref = &file;
        let _ = writeln!(file_ref, "pid={}", std::process::id());
    }
    Ok(file)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_db_path_is_absolute_and_per_app() {
        let path = default_storage_db_path("myapp");
        assert!(path.is_absolute(), "default must not depend on cwd");
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some("myapp.db"));
    }

    #[test]
    fn explicit_db_path_wins_over_default() {
        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: "/data/custom.db".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: Some("myapp".into()),
        };
        assert_eq!(storage_db_path(&config), PathBuf::from("/data/custom.db"));
    }

    #[test]
    fn memory_paths_need_no_lock() {
        assert!(is_memory_db_path(Path::new(":memory:")));
        assert!(is_memory_db_path(Path::new("file::memory:")));
        assert!(!is_memory_db_path(Path::new("/data/app.db")));
    }

    #[test]
    fn lock_path_is_sibling_of_db() {
        let db = Path::new("/data/app.db");
        assert_eq!(storage_lock_path(db), PathBuf::from("/data/app.db.lock"));
    }

    #[test]
    fn second_guard_on_same_db_fails() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("app.db");
        let _first = acquire_storage_lock(&db).expect("first guard acquires");
        let second = acquire_storage_lock(&db);
        assert!(second.is_err(), "second guard must fail, got {second:?}");
    }

    #[test]
    fn guards_on_different_dbs_do_not_interfere() {
        let dir = tempfile::tempdir().expect("tempdir");
        let _first = acquire_storage_lock(&dir.path().join("a.db")).expect("first acquires");
        let _second = acquire_storage_lock(&dir.path().join("b.db")).expect("other db acquires");
    }

    #[test]
    fn guard_releases_on_drop() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("app.db");
        {
            let _guard = acquire_storage_lock(&db).expect("guard acquires");
        }
        let _again = acquire_storage_lock(&db).expect("reacquires after drop");
    }
}
