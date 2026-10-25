use checkpoint_base::error::CheckpointError;

use super::repository::StorageResult;

pub const MIGRATION_SQL: &str = "
CREATE TABLE IF NOT EXISTS meta_kv (
    key             TEXT PRIMARY KEY,
    value           BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);
";

pub const GIT_META_MIGRATION_SQL: &str = "
-- Explicit review state for refs/wf/review/* (replaces implicit history-length checks)
CREATE TABLE IF NOT EXISTS review_state (
    review_ref      TEXT PRIMARY KEY,
    status          TEXT NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Empty-directory manifests keyed by commit id (object store tracks no empty dirs)
CREATE TABLE IF NOT EXISTS empty_dirs (
    commit_id       TEXT PRIMARY KEY,
    dirs            BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Oversized-file manifests keyed by commit id: paths excluded because they
-- exceeded the content cap. Opaque to capture and restore (never deleted).
CREATE TABLE IF NOT EXISTS oversized_files (
    commit_id       TEXT PRIMARY KEY,
    paths           BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Query-acceleration index: commit -> actor/session/tool/paths/timestamp.
-- Lossy by design: dropping every row is safe, rebuild from the graph.
CREATE TABLE IF NOT EXISTS source_index (
    commit_id       TEXT PRIMARY KEY,
    actor           TEXT NOT NULL DEFAULT '',
    session         TEXT NOT NULL DEFAULT '',
    tool            TEXT NOT NULL DEFAULT '',
    paths           BLOB NOT NULL,
    timestamp       INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_source_index_actor ON source_index(actor, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_source_index_timestamp ON source_index(timestamp DESC);

-- Per-path fan-out for path-scoped queries with rename-agnostic LIKE fallback.
CREATE TABLE IF NOT EXISTS source_index_paths (
    commit_id       TEXT NOT NULL,
    path            TEXT NOT NULL,
    PRIMARY KEY (commit_id, path)
);

CREATE INDEX IF NOT EXISTS idx_source_index_paths_path ON source_index_paths(path);

-- Per-actor redo history: undone commit ids, newest last. Persisted so an
-- undone edit can still be redone after a restart. Capped in memory; the
-- stored blob is the whole stack for the actor.
CREATE TABLE IF NOT EXISTS redo_stacks (
    actor           TEXT PRIMARY KEY,
    entries         BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);

DROP TABLE IF EXISTS input_snapshots;
";

pub const PRAGMA_JOURNAL_MODE_WAL: &str = "PRAGMA journal_mode=WAL;";

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

fn apply_light_migrations(conn: &rusqlite::Connection) -> StorageResult<()> {
    conn.execute_batch(GIT_META_MIGRATION_SQL).map_err(db_err)?;
    Ok(())
}

pub fn initialize_database(conn: &rusqlite::Connection) -> StorageResult<()> {
    conn.execute_batch(PRAGMA_JOURNAL_MODE_WAL)
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA journal_size_limit = 67108864;")
        .map_err(db_err)?;
    conn.execute_batch("PRAGMA wal_autocheckpoint = 1000;")
        .map_err(db_err)?;
    conn.execute_batch(MIGRATION_SQL).map_err(db_err)?;
    apply_light_migrations(conn)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table_names(conn: &rusqlite::Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("sqlite_master query");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("iterate tables")
            .collect::<Result<Vec<_>, _>>()
            .expect("table names")
    }

    #[test]
    fn initialize_database_is_idempotent_and_creates_all_tables() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        initialize_database(&conn).unwrap();
        // Re-running against an initialized database must be a no-op success.
        initialize_database(&conn).unwrap();

        let tables = table_names(&conn);
        for expected in [
            "meta_kv",
            "review_state",
            "empty_dirs",
            "oversized_files",
            "source_index",
            "source_index_paths",
            "redo_stacks",
        ] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
        assert!(
            !tables.contains(&"input_snapshots".to_string()),
            "input_snapshots table must be gone"
        );
    }

    #[test]
    fn apply_light_migrations_creates_git_tables_alone() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        apply_light_migrations(&conn).unwrap();
        apply_light_migrations(&conn).unwrap();

        let tables = table_names(&conn);
        for expected in ["review_state", "empty_dirs", "source_index", "redo_stacks"] {
            assert!(
                tables.contains(&expected.to_string()),
                "missing table {expected}"
            );
        }
        // The light path does not create the metadata table.
        assert!(!tables.contains(&"meta_kv".to_string()));
    }

    #[test]
    fn file_database_uses_wal_journal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let storage =
            crate::storage::SqliteStorage::new_full(&dir.path().join("checkpoint.db")).unwrap();
        let mode = storage
            .with_conn(|conn| {
                conn.query_row("PRAGMA journal_mode;", [], |row| row.get::<_, String>(0))
                    .map_err(db_err)
            })
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
