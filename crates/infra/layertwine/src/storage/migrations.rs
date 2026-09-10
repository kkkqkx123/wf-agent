/// Database Migration - Create All Tables
///
pub const MIGRATION_SQL: &str = "
-- File Node Table (Immutable, INSERT ONLY)
CREATE TABLE IF NOT EXISTS file_nodes (
    file_path    TEXT NOT NULL,
    base_hash    BLOB NOT NULL,
    content      BLOB NOT NULL,
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (file_path, base_hash)
);

-- Delta Table (Immutable, INSERT ONLY)
CREATE TABLE IF NOT EXISTS deltas (
    id           BLOB PRIMARY KEY,
    file_path    TEXT NOT NULL,
    file_hash    BLOB NOT NULL,
    diff         BLOB NOT NULL,
    source       TEXT NOT NULL,
    source_data  TEXT,
    timestamp    INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    content_hash BLOB,
    message      TEXT
);

-- Snapshot Table (Immutable, INSERT ONLY)
CREATE TABLE IF NOT EXISTS snapshots (
    id              BLOB PRIMARY KEY,
    file_path       TEXT NOT NULL,
    file_hash       BLOB NOT NULL,
    deltas          BLOB NOT NULL,
    parents         BLOB NOT NULL,
    partition_type  TEXT NOT NULL,
    created_at      INTEGER NOT NULL,
    has_conflicts   INTEGER NOT NULL DEFAULT 0,
    source          TEXT DEFAULT '',
    content_type    TEXT DEFAULT 'file',
    content         BLOB,
    compression     TEXT DEFAULT 'none',
    content_hash    BLOB,
    message         TEXT
);

-- Partition Table
CREATE TABLE IF NOT EXISTS partitions (
    id              BLOB PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    current_snapshot BLOB NOT NULL,
    partition_data  TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Partition History Snapshot Correlation Table
CREATE TABLE IF NOT EXISTS partition_history (
    partition_id    BLOB NOT NULL,
    snapshot_id     BLOB NOT NULL,
    seq             INTEGER NOT NULL,
    created_at      INTEGER NOT NULL,
    PRIMARY KEY (partition_id, seq),
    FOREIGN KEY (partition_id) REFERENCES partitions(id)
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_snapshots_file_created ON snapshots(file_path, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_snapshots_partition_created ON snapshots(partition_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_snapshots_source ON snapshots(source);
CREATE INDEX IF NOT EXISTS idx_snapshots_created_at ON snapshots(created_at);
CREATE INDEX IF NOT EXISTS idx_deltas_file ON deltas(file_path);
CREATE INDEX IF NOT EXISTS idx_deltas_file_timestamp ON deltas(file_path, timestamp);
CREATE INDEX IF NOT EXISTS idx_deltas_timestamp ON deltas(timestamp);
CREATE INDEX IF NOT EXISTS idx_deltas_content_hash ON deltas(content_hash);
CREATE INDEX IF NOT EXISTS idx_partition_history_snapshot ON partition_history(snapshot_id);
CREATE INDEX IF NOT EXISTS idx_snapshots_content_hash ON snapshots(content_hash);

-- Edit Session Table (groups deltas from a single user/Agent operation)
CREATE TABLE IF NOT EXISTS edit_sessions (
    id              BLOB PRIMARY KEY,
    label           TEXT,
    created_at      INTEGER NOT NULL
);

-- Delta-Session association table
CREATE TABLE IF NOT EXISTS delta_sessions (
    delta_id        BLOB NOT NULL,
    session_id      BLOB NOT NULL,
    seq             INTEGER NOT NULL,
    PRIMARY KEY (delta_id, session_id),
    FOREIGN KEY (delta_id) REFERENCES deltas(id),
    FOREIGN KEY (session_id) REFERENCES edit_sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_delta_sessions_session ON delta_sessions(session_id);

-- Snapshot-Session association table (covers full-content snapshots that
-- carry no delta, so a session rollback can find every snapshot it produced)
CREATE TABLE IF NOT EXISTS snapshot_sessions (
    snapshot_id     BLOB NOT NULL,
    session_id      BLOB NOT NULL,
    seq             INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, session_id),
    FOREIGN KEY (snapshot_id) REFERENCES snapshots(id),
    FOREIGN KEY (session_id) REFERENCES edit_sessions(id)
);

CREATE INDEX IF NOT EXISTS idx_snapshot_sessions_session ON snapshot_sessions(session_id);

-- File Move/Rename Tracking Table
CREATE TABLE IF NOT EXISTS file_moves (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    from_path   TEXT NOT NULL,
    to_path     TEXT NOT NULL,
    timestamp   INTEGER NOT NULL,
    source      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_moves_from ON file_moves(from_path);
CREATE INDEX IF NOT EXISTS idx_file_moves_to ON file_moves(to_path);
CREATE INDEX IF NOT EXISTS idx_file_moves_timestamp ON file_moves(timestamp);

-- Single-truth pointer guarantee: whenever a history row is appended the
-- partition's current_snapshot is kept in lockstep, so the DB itself ensures
-- current_snapshot is always the tail of partition_history.
CREATE TRIGGER IF NOT EXISTS trg_partition_current_from_history
AFTER INSERT ON partition_history
BEGIN
    UPDATE partitions
       SET current_snapshot = NEW.snapshot_id,
           updated_at = NEW.created_at
     WHERE id = NEW.partition_id;
END;
";

/// Checkpoint correlation table
pub const MIGRATION_CHECKPOINT_SQL: &str = "
CREATE TABLE IF NOT EXISTS checkpoints (
    id              BLOB PRIMARY KEY,
    parents         BLOB NOT NULL,
    snapshot_ids    BLOB NOT NULL,
    author          TEXT NOT NULL,
    message         TEXT NOT NULL,
    git_anchor      TEXT,
    created_at      INTEGER NOT NULL,
    snapshot_sources TEXT
);

CREATE TABLE IF NOT EXISTS branches (
    name            TEXT PRIMARY KEY,
    head            BLOB NOT NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS meta_kv (
    key             TEXT PRIMARY KEY,
    value           BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_created ON checkpoints(created_at DESC);
";

/// WAL journal mode pragma shared by the core database and standalone
/// backup databases.
pub const PRAGMA_JOURNAL_MODE_WAL: &str = "PRAGMA journal_mode=WAL;";

/// Backup database schema: physically isolated snapshots with SQL-level
/// metadata filtering. Shared so BackupRepo and any future backup tooling
/// converge on a single table definition.
pub const BACKUP_MIGRATION_SQL: &str = "
CREATE TABLE IF NOT EXISTS backup_snapshots (
    id              BLOB PRIMARY KEY,
    source_snapshot BLOB NOT NULL,
    file_path       TEXT NOT NULL,
    file_hash       BLOB NOT NULL,
    deltas          BLOB NOT NULL,
    label           TEXT,
    backed_at       INTEGER NOT NULL,
    metadata        BLOB NOT NULL,
    agent_id        TEXT,
    source_type     TEXT,
    file_content    BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_backup_label ON backup_snapshots(label);
CREATE INDEX IF NOT EXISTS idx_backup_backed_at ON backup_snapshots(backed_at);
CREATE INDEX IF NOT EXISTS idx_backup_agent_id ON backup_snapshots(agent_id);
CREATE INDEX IF NOT EXISTS idx_backup_source_type ON backup_snapshots(source_type);

-- Separate key-value table for SQL-level metadata filtering.
-- Avoids deserializing the JSON blob and filtering in memory.
CREATE TABLE IF NOT EXISTS backup_metadata (
    backup_id BLOB NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (backup_id, key)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_backup_meta_key ON backup_metadata(key, value);
";

/// Idempotent light migrations for databases created before a schema
/// addition. Each statement is best-effort: "duplicate column / already
/// exists" errors are ignored so old files converge to the latest schema.
fn apply_light_migrations(conn: &rusqlite::Connection) -> Result<(), crate::StorageError> {
    // Snapshot message column (added after the initial schema).
    let _ = conn.execute("ALTER TABLE snapshots ADD COLUMN message TEXT", []);
    // Snapshot-session association for full-content snapshots.
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS snapshot_sessions (
            snapshot_id     BLOB NOT NULL,
            session_id      BLOB NOT NULL,
            seq             INTEGER NOT NULL,
            PRIMARY KEY (snapshot_id, session_id),
            FOREIGN KEY (snapshot_id) REFERENCES snapshots(id),
            FOREIGN KEY (session_id) REFERENCES edit_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_snapshot_sessions_session ON snapshot_sessions(session_id);",
    );
    Ok(())
}

/// Initialize the database and apply all migrations
pub fn initialize_database(conn: &rusqlite::Connection) -> Result<(), crate::StorageError> {
    conn.execute_batch(PRAGMA_JOURNAL_MODE_WAL)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    // auto_vacuum=INCREMENTAL: moves freelist pages to end of file for truncation
    // Must be set before any tables are created on a fresh DB.
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
    // Limit WAL journal size to 64MB to prevent unbounded -wal file growth
    conn.execute_batch("PRAGMA journal_size_limit = 67108864;")?;
    // Checkpoint every 1000 pages (default) — explicit for clarity
    conn.execute_batch("PRAGMA wal_autocheckpoint = 1000;")?;
    conn.execute_batch(MIGRATION_SQL)?;
    apply_light_migrations(conn)?;
    Ok(())
}

/// Apply full migration (with checkpoint related tables)
pub fn initialize_full(conn: &rusqlite::Connection) -> Result<(), crate::StorageError> {
    initialize_database(conn)?;
    conn.execute_batch(MIGRATION_CHECKPOINT_SQL)?;
    Ok(())
}
