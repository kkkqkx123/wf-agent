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

use super::types::{BackendEntry, BackupRecordView, SyncStateView};

const REGISTRY_TABLE: &str = "wf_backends";
const BACKUP_TABLE: &str = "wf_backup_index";
const SYNC_STATE_TABLE: &str = "wf_sync_state";

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
    sync_states: DashMap<(String, String), SyncStateView>,
}

#[derive(Debug, Clone)]
pub struct SqliteBackendStore {
    pool: sqlx::SqlitePool,
}

type BackupRow = (
    String,
    String,
    String,
    i64,
    i64,
    String,
    String,
    i64,
    String,
    String,
);

fn backup_view(row: BackupRow) -> BackupRecordView {
    let (
        id,
        backend,
        execution,
        created_ms,
        size_bytes,
        sha256,
        path,
        source_sequence,
        source_revision,
        payload_digest,
    ) = row;
    BackupRecordView {
        id,
        backend,
        execution,
        created_ms,
        size_bytes: size_bytes.max(0) as u64,
        sha256,
        path,
        source_sequence: source_sequence.max(0) as u64,
        source_revision,
        payload_digest,
    }
}

type SyncStateRow = (
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    i64,
    String,
    String,
    Option<String>,
);

fn sync_state_view(row: SyncStateRow) -> SyncStateView {
    let (
        backend,
        execution,
        last_success_ms,
        last_attempt_ms,
        revision,
        digest,
        backup_id,
        failures,
        repair_status,
        local_status,
        error,
    ) = row;
    SyncStateView {
        backend,
        execution,
        last_success_ms,
        last_attempt_ms,
        revision: revision.map(|value| value.max(0) as u64),
        digest,
        backup_id,
        failures: failures.max(0) as u32,
        repair_status,
        local_status,
        error,
    }
}

impl BackendStore {
    pub fn new_memory() -> Self {
        Self::Memory(Arc::new(MemoryBackendStore::default()))
    }

    /// Open a sqlite-backed store, creating both tables when missing.
    /// A present but corrupt database fails loud so a broken registry
    /// cannot masquerade as an empty one.
    pub async fn new_sqlite(pool: sqlx::SqlitePool) -> Result<Self, String> {
        for table in [REGISTRY_TABLE, BACKUP_TABLE, SYNC_STATE_TABLE] {
            let create = if table == REGISTRY_TABLE {
                format!(
                    "CREATE TABLE IF NOT EXISTS {table} (
                        name TEXT PRIMARY KEY,
                        entry_json TEXT NOT NULL,
                        updated_ms INTEGER NOT NULL
                    )"
                )
            } else if table == BACKUP_TABLE {
                format!(
                    "CREATE TABLE IF NOT EXISTS {table} (
                        id TEXT PRIMARY KEY,
                        backend TEXT NOT NULL,
                        execution TEXT NOT NULL,
                        created_ms INTEGER NOT NULL,
                        size_bytes INTEGER NOT NULL,
                        sha256 TEXT NOT NULL,
                        path TEXT NOT NULL,
                        source_sequence INTEGER NOT NULL,
                        source_revision TEXT NOT NULL,
                        payload_digest TEXT NOT NULL
                    )"
                )
            } else {
                format!(
                    "CREATE TABLE IF NOT EXISTS {table} (
                        backend TEXT NOT NULL,
                        execution TEXT NOT NULL,
                        last_success_ms INTEGER,
                        last_attempt_ms INTEGER,
                        revision INTEGER,
                        digest TEXT,
                        backup_id TEXT,
                        failures INTEGER NOT NULL,
                        repair_status TEXT NOT NULL,
                        local_status TEXT NOT NULL DEFAULT 'unverified',
                        error TEXT,
                        PRIMARY KEY (backend, execution)
                    )"
                )
            };
            sqlx::query(&create)
                .execute(&pool)
                .await
                .map_err(|e| format!("cannot init backend table {table}: {e}"))?;
        }
        // The sync state table is extended in place: an existing database
        // keeps its rows and gains the local readback column.
        let columns: Vec<(String,)> = sqlx::query_as(&format!(
            "SELECT name FROM pragma_table_info('{SYNC_STATE_TABLE}')"
        ))
        .fetch_all(&pool)
        .await
        .map_err(|e| format!("cannot inspect sync state columns: {e}"))?;
        if !columns.iter().any(|(name,)| name == "local_status") {
            sqlx::query(&format!(
                "ALTER TABLE {SYNC_STATE_TABLE}
                 ADD COLUMN local_status TEXT NOT NULL DEFAULT 'unverified'"
            ))
            .execute(&pool)
            .await
            .map_err(|e| format!("cannot add sync state local column: {e}"))?;
        }
        // Pruning and due-time lookups always filter by (backend, execution),
        // so the same pair is indexed with the sort key pruning needs.
        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS idx_{BACKUP_TABLE}_target
             ON {BACKUP_TABLE} (backend, execution, created_ms DESC)"
        ))
        .execute(&pool)
        .await
        .map_err(|e| format!("cannot init backup index: {e}"))?;
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
        sqlx::query(&format!(
            "CREATE INDEX IF NOT EXISTS idx_{SYNC_STATE_TABLE}_target
             ON {SYNC_STATE_TABLE} (backend, execution)"
        ))
        .execute(&pool)
        .await
        .map_err(|e| format!("cannot init sync state index: {e}"))?;
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
                    "INSERT INTO {BACKUP_TABLE} (id, backend, execution, created_ms, size_bytes, sha256, path, source_sequence, source_revision, payload_digest)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(id) DO UPDATE SET backend = excluded.backend, execution = excluded.execution,
                       created_ms = excluded.created_ms, size_bytes = excluded.size_bytes,
                       sha256 = excluded.sha256, path = excluded.path,
                       source_sequence = excluded.source_sequence,
                       source_revision = excluded.source_revision, payload_digest = excluded.payload_digest"
                ))
                .bind(&record.id)
                .bind(&record.backend)
                .bind(&record.execution)
                .bind(record.created_ms)
                .bind(record.size_bytes as i64)
                .bind(&record.sha256)
                .bind(&record.path)
                .bind(record.source_sequence as i64)
                .bind(&record.source_revision)
                .bind(&record.payload_digest)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot record backup '{}': {e}", record.id))?;
                Ok(())
            }
        }
    }

    /// Rewrite only the stored path of one backup row. Relinking never
    /// touches file bytes, sizes or digests.
    pub async fn update_backup_path(&self, id: &str, path: &str) -> Result<bool, String> {
        match self {
            Self::Memory(inner) => match inner.backups.get_mut(id) {
                Some(mut row) => {
                    row.path = path.to_string();
                    Ok(true)
                }
                None => Ok(false),
            },
            Self::Sqlite(inner) => {
                let result = sqlx::query(&format!(
                    "UPDATE {BACKUP_TABLE} SET path = ?1 WHERE id = ?2"
                ))
                .bind(path)
                .bind(id)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot relink backup '{id}': {e}"))?;
                Ok(result.rows_affected() > 0)
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
                let rows: Vec<BackupRow> = if let Some(backend) = backend {
                    sqlx::query_as(&format!(
                        "SELECT id, backend, execution, created_ms, size_bytes, sha256, path, source_sequence, source_revision, payload_digest
                         FROM {BACKUP_TABLE} WHERE backend = ?1 ORDER BY created_ms DESC"
                    ))
                    .bind(backend)
                    .fetch_all(&inner.pool)
                    .await
                    .map_err(|e| format!("cannot list backups: {e}"))?
                } else {
                    sqlx::query_as(&format!(
                        "SELECT id, backend, execution, created_ms, size_bytes, sha256, path, source_sequence, source_revision, payload_digest
                         FROM {BACKUP_TABLE} ORDER BY created_ms DESC"
                    ))
                    .fetch_all(&inner.pool)
                    .await
                    .map_err(|e| format!("cannot list backups: {e}"))?
                };
                rows.into_iter().map(backup_view).collect()
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

    /// Backups of one backup target (backend plus execution), newest first.
    /// Retention is scoped per target, so pruning never competes with a
    /// sibling execution's history.
    pub async fn list_backups_for(
        &self,
        backend: &str,
        execution: &str,
    ) -> Result<Vec<BackupRecordView>, String> {
        match self {
            Self::Memory(inner) => {
                let mut records = inner
                    .backups
                    .iter()
                    .filter(|row| row.backend == backend && row.execution == execution)
                    .map(|row| row.value().clone())
                    .collect::<Vec<_>>();
                records.sort_by_key(|record| std::cmp::Reverse(record.created_ms));
                Ok(records)
            }
            Self::Sqlite(inner) => {
                let rows: Vec<BackupRow> = sqlx::query_as(&format!(
                    "SELECT id, backend, execution, created_ms, size_bytes, sha256, path, source_sequence, source_revision, payload_digest
                     FROM {BACKUP_TABLE} WHERE backend = ?1 AND execution = ?2
                     ORDER BY created_ms DESC"
                ))
                .bind(backend)
                .bind(execution)
                .fetch_all(&inner.pool)
                .await
                .map_err(|e| format!("cannot list backups for '{backend}/{execution}': {e}"))?;
                Ok(rows.into_iter().map(backup_view).collect())
            }
        }
    }

    /// Newest recorded backup time of one target. Used as the durable
    /// fallback for due-time decisions so a restart cannot make every
    /// target due at once.
    pub async fn latest_backup_ms(
        &self,
        backend: &str,
        execution: &str,
    ) -> Result<Option<i64>, String> {
        match self {
            Self::Memory(inner) => Ok(inner
                .backups
                .iter()
                .filter(|row| row.backend == backend && row.execution == execution)
                .map(|row| row.created_ms)
                .max()),
            Self::Sqlite(inner) => {
                let row: Option<(Option<i64>,)> = sqlx::query_as(&format!(
                    "SELECT MAX(created_ms) FROM {BACKUP_TABLE} WHERE backend = ?1 AND execution = ?2"
                ))
                .bind(backend)
                .bind(execution)
                .fetch_optional(&inner.pool)
                .await
                .map_err(|e| format!("cannot read latest backup for '{backend}/{execution}': {e}"))?;
                Ok(row.and_then(|(value,)| value))
            }
        }
    }

    pub async fn get_backup(&self, id: &str) -> Result<Option<BackupRecordView>, String> {
        match self {
            Self::Memory(inner) => Ok(inner.backups.get(id).map(|row| row.value().clone())),
            Self::Sqlite(inner) => {
                let row: Option<BackupRow> =
                    sqlx::query_as(&format!(
                        "SELECT id, backend, execution, created_ms, size_bytes, sha256, path, source_sequence, source_revision, payload_digest
                         FROM {BACKUP_TABLE} WHERE id = ?1"
                    ))
                    .bind(id)
                    .fetch_optional(&inner.pool)
                    .await
                    .map_err(|e| format!("cannot read backup '{id}': {e}"))?;
                Ok(row.map(backup_view))
            }
        }
    }

    pub async fn upsert_sync_state(&self, state: &SyncStateView) -> Result<(), String> {
        match self {
            Self::Memory(inner) => {
                inner.sync_states.insert(
                    (state.backend.clone(), state.execution.clone()),
                    state.clone(),
                );
                Ok(())
            }
            Self::Sqlite(inner) => {
                sqlx::query(&format!(
                    "INSERT INTO {SYNC_STATE_TABLE} (backend, execution, last_success_ms, last_attempt_ms, revision, digest, backup_id, failures, repair_status, local_status, error)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                     ON CONFLICT(backend, execution) DO UPDATE SET
                       last_success_ms = excluded.last_success_ms,
                       last_attempt_ms = excluded.last_attempt_ms,
                       revision = excluded.revision,
                       digest = excluded.digest,
                       backup_id = excluded.backup_id,
                       failures = excluded.failures,
                       repair_status = excluded.repair_status,
                       local_status = excluded.local_status,
                       error = excluded.error"
                ))
                .bind(&state.backend)
                .bind(&state.execution)
                .bind(state.last_success_ms)
                .bind(state.last_attempt_ms)
                .bind(state.revision.map(|value| value as i64))
                .bind(&state.digest)
                .bind(&state.backup_id)
                .bind(state.failures as i64)
                .bind(&state.repair_status)
                .bind(&state.local_status)
                .bind(&state.error)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot record sync state: {e}"))?;
                Ok(())
            }
        }
    }

    pub async fn get_sync_state(
        &self,
        backend: &str,
        execution: &str,
    ) -> Result<Option<SyncStateView>, String> {
        match self {
            Self::Memory(inner) => Ok(inner
                .sync_states
                .get(&(backend.to_string(), execution.to_string()))
                .map(|row| row.value().clone())),
            Self::Sqlite(inner) => {
                let row: Option<SyncStateRow> = sqlx::query_as(&format!(
                    "SELECT backend, execution, last_success_ms, last_attempt_ms, revision, digest, backup_id, failures, repair_status, local_status, error
                     FROM {SYNC_STATE_TABLE} WHERE backend = ?1 AND execution = ?2"
                ))
                .bind(backend)
                .bind(execution)
                .fetch_optional(&inner.pool)
                .await
                .map_err(|e| format!("cannot read sync state: {e}"))?;
                Ok(row.map(sync_state_view))
            }
        }
    }

    pub async fn list_sync_states(&self) -> Result<Vec<SyncStateView>, String> {
        match self {
            Self::Memory(inner) => Ok(inner
                .sync_states
                .iter()
                .map(|row| row.value().clone())
                .collect()),
            Self::Sqlite(inner) => {
                let rows: Vec<SyncStateRow> = sqlx::query_as(&format!(
                    "SELECT backend, execution, last_success_ms, last_attempt_ms, revision, digest, backup_id, failures, repair_status, local_status, error
                     FROM {SYNC_STATE_TABLE}"
                ))
                .fetch_all(&inner.pool)
                .await
                .map_err(|e| format!("cannot list sync states: {e}"))?;
                Ok(rows.into_iter().map(sync_state_view).collect())
            }
        }
    }

    pub async fn remove_sync_state(&self, backend: &str, execution: &str) -> Result<(), String> {
        match self {
            Self::Memory(inner) => {
                inner
                    .sync_states
                    .remove(&(backend.to_string(), execution.to_string()));
                Ok(())
            }
            Self::Sqlite(inner) => {
                sqlx::query(&format!(
                    "DELETE FROM {SYNC_STATE_TABLE} WHERE backend = ?1 AND execution = ?2"
                ))
                .bind(backend)
                .bind(execution)
                .execute(&inner.pool)
                .await
                .map_err(|e| format!("cannot delete sync state: {e}"))?;
                Ok(())
            }
        }
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
                backup_directory: None,
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
            source_sequence: 1,
            source_revision: "rev".to_string(),
            payload_digest: "rev".to_string(),
        };
        store.record_backup(&record).await.unwrap();
        let listed = store.list_backups(Some("alpha")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(store.remove_backup("backup-1").await.unwrap());
    }

    #[tokio::test]
    async fn sync_state_round_trip_and_remove() {
        let store = BackendStore::new_memory();
        let state = SyncStateView {
            backend: "alpha".to_string(),
            execution: "exec-1".to_string(),
            last_success_ms: Some(10),
            last_attempt_ms: Some(12),
            revision: Some(7),
            digest: Some("rev".to_string()),
            backup_id: Some("backup-1".to_string()),
            failures: 2,
            repair_status: "repair_pending".to_string(),
            local_status: "missing_file".to_string(),
            error: Some("down".to_string()),
        };
        store.upsert_sync_state(&state).await.unwrap();
        let loaded = store
            .get_sync_state("alpha", "exec-1")
            .await
            .unwrap()
            .expect("sync state row");
        assert_eq!(loaded.revision, Some(7));
        assert_eq!(loaded.digest.as_deref(), Some("rev"));
        assert_eq!(loaded.failures, 2);
        assert_eq!(store.list_sync_states().await.unwrap().len(), 1);
        store.remove_sync_state("alpha", "exec-1").await.unwrap();
        assert!(store.list_sync_states().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn legacy_sync_state_table_gains_the_local_column() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        // Table shape from before the local readback column existed.
        sqlx::query(
            "CREATE TABLE wf_sync_state (
                backend TEXT NOT NULL,
                execution TEXT NOT NULL,
                last_success_ms INTEGER,
                last_attempt_ms INTEGER,
                revision INTEGER,
                digest TEXT,
                backup_id TEXT,
                failures INTEGER NOT NULL,
                repair_status TEXT NOT NULL,
                error TEXT,
                PRIMARY KEY (backend, execution)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO wf_sync_state (backend, execution, last_success_ms, last_attempt_ms, revision, digest, backup_id, failures, repair_status, error)
             VALUES ('alpha', 'exec-1', 10, 12, 3, 'rev', 'backup-1', 1, 'remote_rollback', 'moved back')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let store = BackendStore::new_sqlite(pool).await.unwrap();
        let state = store
            .get_sync_state("alpha", "exec-1")
            .await
            .unwrap()
            .expect("legacy row survives the migration");
        assert_eq!(state.revision, Some(3));
        assert_eq!(state.repair_status, "remote_rollback");
        assert_eq!(state.local_status, "unverified");
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
