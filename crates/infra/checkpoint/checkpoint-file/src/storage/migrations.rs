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

pub fn initialize_full(conn: &rusqlite::Connection) -> StorageResult<()> {
    initialize_database(conn)?;
    Ok(())
}
