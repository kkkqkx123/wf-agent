//! Backend registry storage: one JSON row per host plus an append-only
//! backup index. Follows the existing `CREATE TABLE IF NOT EXISTS`
//! initialization pattern; the memory backend mirrors the same interface
//! with process-local maps.
//!
//! Runtime liveness (reachability, tunnel pids) is never persisted: the
//! manager keeps it in memory and reports it beside the stored reference.

use std::collections::BTreeMap;
use std::sync::Arc;

use dashmap::DashMap;

use super::types::{BackendEntry, BackupRecordView};

const REGISTRY_TABLE: &str = "wf_backends";
const BACKUP_TABLE: &str = "wf_backup_index";

/// Stored registry row: the whole entry as JSON plus an update timestamp.
/// Host names are the unique key; writes only flow through the manager so
/// the server stays the single writer.
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct StoredRow {
    entry_json: String,
    updated_ms: i64,
}

#[derive(Debug, Clone)]
pub enum BackendStore {
    Memory(Arc<MemoryBackendStore>),
    Sqlite(Arc<SqliteBackendStore>),
}

#[derive(Debug, Default)]
pub struct MemoryBackendStore {
    rows: DashMap<String, StoredRow>,
    backups: DashMap<String, BackupRecordView>,
}

#[derive(Debug, Clone)]
pub struct SqliteBackendStore {
    pool: sqlx::SqlitePool,
}

impl BackendStore {
    pub fn new_memory() -> Self {
        Self::Memory(Arc::new(MemoryBackendStore::default()))
    }

    /// Open a sqlite-backed store, creating both tables when missing.
    /// A present but corrupt database fails loud so a broken registry
    /// cannot masquerade as an empty one.
    pub async fn new_sqlite(pool: sqlx::SqlitePool) -> Result<Self, String> {
        for table in [REGISTRY_TABLE, BACKUP_TABLE] {
            let create = if table == REGISTRY_TABLE {
                format!(
                    "CREATE TABLE IF NOT EXISTS {table} (
                        name TEXT PRIMARY KEY,
                        entry_json TEXT NOT NULL,
                        updated_ms INTEGER NOT NULL
                    )"
                )
            } else {
                format!(
                    "CREATE TABLE IF NOT EXISTS {table} (
                        id TEXT PRIMARY KEY,
                        backend TEXT NOT NULL,
                        execution TEXT NOT NULL,
                        created_ms INTEGER NOT NULL,
                        size_bytes INTEGER NOT NULL,
                        sha256 TEXT NOT NULL,
                        path TEXT NOT NULL
                    )"
                )
            };
            sqlx::query(&create)
                .execute(&pool)
                .await
                .map_err(|e| format!("cannot init backend table {table}: {e}"))?;
        }
        // Corrupt JSON rows fail here instead of surfacing as empty later.
        let rows: Vec<(String, String)> =
            sqlx::query_as(&format!("SELECT name, entry_json FROM {REGISTRY_TABLE}"))
                .fetch_all(&pool)
                .await
                .map_err(|e| format!("cannot read backend registry: {e}"))?;
        for (name, raw) in rows {
            serde_json::from_str::<BackendEntry>(&raw)
                .map_err(|e| format!("invalid backend registry row '{name}': {e}"))?;
        }
        Ok(Self::Sqlite(Arc::new(SqliteBackendStore { pool })))
    }

    /// Open a sqlite-backed store from a file path, creating parent
    /// directories. Used when the runtime storage is file-backed.
    pub async fn open_sqlite_file(path: &std::path::Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create backend dir {}: {e}", parent.display()))?;
            }
        }
        let url = format!("sqlite:{}?mode=rwc", path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await
            .map_err(|e| format!("cannot open backend database {}: {e}", path.display()))?;
        Self::new_sqlite(pool).await
    }

    pub async fn list_entries(&self) -> Result<BTreeMap<String, BackendEntry>, String> {
        match self {
            Self::Memory(inner) => {
                let mut out = BTreeMap::new();
                for row in inner.rows.iter() {
                    let entry: BackendEntry =
                        serde_json::from_str(&row.entry_json).map_err(|e| {
                            format!("invalid backend registry row '{}': {e}", row.key())
                        })?;
                    out.insert(row.key().clone(), entry);
                }
                Ok(out)
            }
            Self::Sqlite(inner) => {
                let rows: Vec<(String, String)> =
                    sqlx::query_as(&format!("SELECT name, entry_json FROM {REGISTRY_TABLE}"))
                        .fetch_all(&inner.pool)
                        .await
                        .map_err(|e| format!("cannot list backend registry: {e}"))?;
                let mut out = BTreeMap::new();
                for (name, raw) in rows {
                    let entry: BackendEntry = serde_json::from_str(&raw)
                        .map_err(|e| format!("invalid backend registry row '{name}': {e}"))?;
                    out.insert(name, entry);
                }
                Ok(out)
            }
        }
    }

    pub async fn get_entry(&self, name: &str) -> Result<Option<BackendEntry>, String> {
        match self {
            Self::Memory(inner) => match inner.rows.get(name) {
                None => Ok(None),
                Some(row) => {
                    let entry: BackendEntry = serde_json::from_str(&row.entry_json)
                        .map_err(|e| format!("invalid backend registry row '{name}': {e}"))?;
                    Ok(Some(entry))
                }
            },
            Self::Sqlite(inner) => {
                let row: Option<(String,)> = sqlx::query_as(&format!(
                    "SELECT entry_json FROM {REGISTRY_TABLE} WHERE name = ?1"
                ))
                .bind(name)
                .fetch_optional(&inner.pool)
                .await
                .map_err(|e| format!("cannot read backend '{name}': {e}"))?;
                match row {
                    None => Ok(None),
                    Some((raw,)) => {
                        let entry: BackendEntry = serde_json::from_str(&raw)
                            .map_err(|e| format!("invalid backend registry row '{name}': {e}"))?;
                        Ok(Some(entry))
                    }
                }
            }
        }
    }

    pub async fn put_entry(&self, name: &str, entry: &BackendEntry) -> Result<(), String> {
        let raw = serde_json::to_string(entry)
            .map_err(|e| format!("cannot encode backend '{name}': {e}"))?;
        let now = wf_common::epoch_ms();
        match self {
            Self::Memory(inner) => {
                inner.rows.insert(
                    name.to_string(),
                    StoredRow {
                        entry_json: raw,
                        updated_ms: now,
                    },
                );
                Ok(())
            }
            Self::Sqlite(inner) => {
                sqlx::query(&format!(
                    "INSERT INTO {REGISTRY_TABLE} (name, entry_json, updated_ms)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(name) DO UPDATE SET entry_json = excluded.entry_json, updated_ms = excluded.updated_ms"
                ))
                .bind(name)
                .bind(&raw)
                .bind(now)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot write backend '{name}': {e}"))?;
                Ok(())
            }
        }
    }

    pub async fn remove_entry(&self, name: &str) -> Result<bool, String> {
        match self {
            Self::Memory(inner) => Ok(inner.rows.remove(name).is_some()),
            Self::Sqlite(inner) => {
                let result = sqlx::query(&format!("DELETE FROM {REGISTRY_TABLE} WHERE name = ?1"))
                    .bind(name)
                    .execute(&inner.pool)
                    .await
                    .map_err(|e| format!("cannot delete backend '{name}': {e}"))?;
                Ok(result.rows_affected() > 0)
            }
        }
    }

    pub async fn record_backup(&self, record: &BackupRecordView) -> Result<(), String> {
        match self {
            Self::Memory(inner) => {
                inner.backups.insert(record.id.clone(), record.clone());
                Ok(())
            }
            Self::Sqlite(inner) => {
                sqlx::query(&format!(
                    "INSERT INTO {BACKUP_TABLE} (id, backend, execution, created_ms, size_bytes, sha256, path)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT(id) DO UPDATE SET backend = excluded.backend, execution = excluded.execution,
                       created_ms = excluded.created_ms, size_bytes = excluded.size_bytes,
                       sha256 = excluded.sha256, path = excluded.path"
                ))
                .bind(&record.id)
                .bind(&record.backend)
                .bind(&record.execution)
                .bind(record.created_ms)
                .bind(record.size_bytes as i64)
                .bind(&record.sha256)
                .bind(&record.path)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot record backup '{}': {e}", record.id))?;
                Ok(())
            }
        }
    }

    pub async fn list_backups(
        &self,
        backend: Option<&str>,
    ) -> Result<Vec<BackupRecordView>, String> {
        let mut records = match self {
            Self::Memory(inner) => inner
                .backups
                .iter()
                .map(|row| row.value().clone())
                .collect::<Vec<_>>(),
            Self::Sqlite(inner) => {
                let rows: Vec<(String, String, String, i64, i64, String, String)> =
                    if let Some(backend) = backend {
                        sqlx::query_as(&format!(
                            "SELECT id, backend, execution, created_ms, size_bytes, sha256, path
                             FROM {BACKUP_TABLE} WHERE backend = ?1 ORDER BY created_ms DESC"
                        ))
                        .bind(backend)
                        .fetch_all(&inner.pool)
                        .await
                        .map_err(|e| format!("cannot list backups: {e}"))?
                    } else {
                        sqlx::query_as(&format!(
                            "SELECT id, backend, execution, created_ms, size_bytes, sha256, path
                             FROM {BACKUP_TABLE} ORDER BY created_ms DESC"
                        ))
                        .fetch_all(&inner.pool)
                        .await
                        .map_err(|e| format!("cannot list backups: {e}"))?
                    };
                rows.into_iter()
                    .map(
                        |(id, backend, execution, created_ms, size_bytes, sha256, path)| {
                            BackupRecordView {
                                id,
                                backend,
                                execution,
                                created_ms,
                                size_bytes: size_bytes.max(0) as u64,
                                sha256,
                                path,
                            }
                        },
                    )
                    .collect()
            }
        };
        if matches!(self, Self::Memory(_)) {
            if let Some(backend) = backend {
                records.retain(|record| record.backend == backend);
            }
            records.sort_by_key(|record| std::cmp::Reverse(record.created_ms));
        }
        Ok(records)
    }

    pub async fn remove_backup(&self, id: &str) -> Result<bool, String> {
        match self {
            Self::Memory(inner) => Ok(inner.backups.remove(id).is_some()),
            Self::Sqlite(inner) => {
                let result = sqlx::query(&format!("DELETE FROM {BACKUP_TABLE} WHERE id = ?1"))
                    .bind(id)
                    .execute(&inner.pool)
                    .await
                    .map_err(|e| format!("cannot delete backup '{id}': {e}"))?;
                Ok(result.rows_affected() > 0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::types::BackendEntry;

    fn sample(name: &str) -> (String, BackendEntry) {
        (
            name.to_string(),
            BackendEntry {
                host: "example".to_string(),
                port: None,
                username: "dev".to_string(),
                key_path: None,
                jump_host: None,
                remote_dir: None,
                server_port: None,
                local_port: None,
                backup_interval_secs: None,
                backup_retention_count: None,
                backup_retention_days: None,
                backup_executions: Vec::new(),
            },
        )
    }

    #[tokio::test]
    async fn memory_round_trip_and_remove() {
        let store = BackendStore::new_memory();
        let (name, entry) = sample("alpha");
        store.put_entry(&name, &entry).await.unwrap();
        let listed = store.list_entries().await.unwrap();
        assert!(listed.contains_key("alpha"));
        assert!(store.remove_entry("alpha").await.unwrap());
        assert!(!store.remove_entry("alpha").await.unwrap());
    }

    #[tokio::test]
    async fn concurrent_writes_keep_last_row() {
        let store = BackendStore::new_memory();
        let mut handles = Vec::new();
        for index in 0..16 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                let (_, mut entry) = sample("shared");
                entry.host = format!("host-{index}");
                let name = "shared".to_string();
                store.put_entry(&name, &entry).await.unwrap();
            }));
        }
        for handle in handles {
            handle.await.unwrap();
        }
        let entry = store.get_entry("shared").await.unwrap().unwrap();
        assert!(entry.host.starts_with("host-"));
    }

    #[tokio::test]
    async fn backup_retention_inputs_round_trip() {
        let store = BackendStore::new_memory();
        let record = BackupRecordView {
            id: "backup-1".to_string(),
            backend: "alpha".to_string(),
            execution: "exec-1".to_string(),
            created_ms: 10,
            size_bytes: 12,
            sha256: "abc".to_string(),
            path: "/tmp/backup-1.json".to_string(),
        };
        store.record_backup(&record).await.unwrap();
        let listed = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(store.remove_backup("backup-1").await.unwrap());
    }

    #[tokio::test]
    async fn corrupt_registry_row_fails_loud() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS wf_backends (name TEXT PRIMARY KEY, entry_json TEXT NOT NULL, updated_ms INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS wf_backup_index (id TEXT PRIMARY KEY, backend TEXT NOT NULL, execution TEXT NOT NULL, created_ms INTEGER NOT NULL, size_bytes INTEGER NOT NULL, sha256 TEXT NOT NULL, path TEXT NOT NULL)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO wf_backends (name, entry_json, updated_ms) VALUES (?1, ?2, ?3)")
            .bind("broken")
            .bind("{not json")
            .bind(1i64)
            .execute(&pool)
            .await
            .unwrap();
        let err = BackendStore::new_sqlite(pool).await.unwrap_err();
        assert!(
            err.contains("invalid backend registry row 'broken'"),
            "{err}"
        );
    }

    #[tokio::test]
    async fn corrupt_database_file_fails_loud() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.db");
        std::fs::write(&path, b"not a sqlite file").unwrap();
        let result = BackendStore::open_sqlite_file(&path).await;
        assert!(result.is_err(), "corrupt db must fail loud");
    }
}
