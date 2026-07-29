/// Database Migration - Create All Tables
///
/// Refer to architecture/07-Rust-implementation.md §7.6 for table building SQL.
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
    created_at   INTEGER NOT NULL
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
    compression     TEXT DEFAULT 'none'
);

-- Partition Table
CREATE TABLE IF NOT EXISTS partitions (
    id              BLOB PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    current_snapshot BLOB NOT NULL,
    partition_type  TEXT NOT NULL,
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

-- Layer Table
CREATE TABLE IF NOT EXISTS layers (
    layer_type      TEXT PRIMARY KEY,
    partition_ids   BLOB NOT NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_snapshots_file_created ON snapshots(file_path, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_snapshots_partition_created ON snapshots(partition_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_snapshots_source ON snapshots(source);
CREATE INDEX IF NOT EXISTS idx_deltas_file ON deltas(file_path);
CREATE INDEX IF NOT EXISTS idx_partition_history_snapshot ON partition_history(snapshot_id);
";

/// Checkpoint correlation table (used in P4)
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

CREATE TABLE IF NOT EXISTS dag_store (
    key             TEXT PRIMARY KEY,
    value           BLOB NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- DAG edge table: stores parent→child relationships
CREATE TABLE IF NOT EXISTS dag_edges (
parent_id BLOB NOT NULL,
child_id BLOB NOT NULL,
PRIMARY KEY (parent_id, child_id)
) WITHOUT ROWID;

-- Generation numbering table: stores the maximum distance of each node (distance from root node)
CREATE TABLE IF NOT EXISTS dag_generations (
node_id BLOB PRIMARY KEY,
generation INTEGER NOT NULL
) WITHOUT ROWID;

-- Index: child_id direction query (reverse traversal)
CREATE INDEX IF NOT EXISTS idx_dag_edges_child ON dag_edges(child_id);

-- Time Index Table (for fast time-based checkpoint queries)
CREATE TABLE IF NOT EXISTS time_index (
    checkpoint_id   BLOB PRIMARY KEY,
    created_at      INTEGER NOT NULL,
    FOREIGN KEY (checkpoint_id) REFERENCES checkpoints(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_created ON checkpoints(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_time_index_created_at ON time_index(created_at);

-- Transaction Log Table (for WAL-based transaction tracking)
CREATE TABLE IF NOT EXISTS transaction_log (
    id              TEXT PRIMARY KEY,
    state           TEXT NOT NULL,
    checkpoints     TEXT,
    created_at      INTEGER NOT NULL
);
";

/// Initialize the database and apply all migrations
pub fn initialize_database(conn: &rusqlite::Connection) -> Result<(), crate::StorageError> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    // auto_vacuum=INCREMENTAL: moves freelist pages to end of file for truncation
    // Must be set before any tables are created on a fresh DB.
    conn.execute_batch("PRAGMA auto_vacuum = INCREMENTAL;")?;
    // Limit WAL journal size to 64MB to prevent unbounded -wal file growth
    conn.execute_batch("PRAGMA journal_size_limit = 67108864;")?;
    // Checkpoint every 1000 pages (default) — explicit for clarity
    conn.execute_batch("PRAGMA wal_autocheckpoint = 1000;")?;
    conn.execute_batch(MIGRATION_SQL)?;
    Ok(())
}

/// Apply full migration (with checkpoint related tables)
pub fn initialize_full(conn: &rusqlite::Connection) -> Result<(), crate::StorageError> {
    initialize_database(conn)?;
    conn.execute_batch(MIGRATION_CHECKPOINT_SQL)?;
    Ok(())
}
