use crate::config::CompactOptions;
use crate::config::CompactReport;
use crate::StorageResult;
use parking_lot::ReentrantMutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

pub struct SqliteStorage {
    pub conn: Arc<ReentrantMutex<Connection>>,
}

impl Clone for SqliteStorage {
    fn clone(&self) -> Self {
        SqliteStorage {
            conn: self.conn.clone(),
        }
    }
}

impl crate::storage::repository::AtomicOps for SqliteStorage {
    fn with_atomic<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Self) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        conn.execute_batch("SAVEPOINT atomic_savepoint;")?;
        match f(self) {
            Ok(value) => {
                conn.execute_batch("RELEASE SAVEPOINT atomic_savepoint;")?;
                drop(conn);
                Ok(value)
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK TO SAVEPOINT atomic_savepoint;")?;
                drop(conn);
                Err(e)
            }
        }
    }
}

impl SqliteStorage {
    pub fn new_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        crate::storage::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_full_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        crate::storage::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path)?;
        crate::storage::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_full(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path)?;
        crate::storage::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_with_connection_arc(conn: &Arc<ReentrantMutex<Connection>>) -> Self {
        SqliteStorage { conn: conn.clone() }
    }

    pub fn share(&self) -> Self {
        SqliteStorage {
            conn: self.conn.clone(),
        }
    }

    pub fn with_conn<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Connection) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }

    /// Run periodic maintenance to prevent file bloat:
    /// 1. Truncate the WAL journal file via checkpoint.
    /// 2. If the freelist ratio exceeds the threshold, reclaim pages
    ///    via incremental_vacuum (or full VACUUM if `vacuum_full` is set).
    ///
    /// Returns a `CompactReport` describing what was done.
    ///
    /// Call this method during idle periods (e.g. after a batch of edits,
    /// or on a timer in long-running processes).
    pub fn run_maintenance(&self) -> StorageResult<CompactReport> {
        self.run_maintenance_with(&CompactOptions::default())
    }

    /// Run maintenance with explicit options from config or overrides.
    pub fn run_maintenance_with(&self, opts: &CompactOptions) -> StorageResult<CompactReport> {
        self.with_conn(|conn| {
            // Step 1: Checkpoint and truncate the WAL file
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

            // Step 2: Check freelist ratio
            let page_count: i64 = conn.query_row("PRAGMA page_count", [], |row| row.get(0))?;
            let freelist_before: i64 =
                conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;

            let vacuum_performed;
            let message;

            if page_count == 0 {
                vacuum_performed = false;
                message = "database is empty, nothing to compact".into();
            } else if opts.vacuum_full {
                conn.execute_batch("VACUUM;")?;
                vacuum_performed = true;
                message = "full VACUUM completed".into();
            } else if freelist_before as f64 / page_count as f64 > opts.freelist_threshold {
                let pages = freelist_before.min(opts.max_vacuum_pages);
                conn.execute(&format!("PRAGMA incremental_vacuum({})", pages), [])?;
                vacuum_performed = true;
                message = format!(
                    "incremental_vacuum: reclaimed up to {} pages (freelist was {}/{})",
                    pages, freelist_before, page_count
                );
            } else {
                vacuum_performed = false;
                message = format!(
                    "freelist ratio {:.2}% below threshold {:.0}%, skipped vacuum",
                    freelist_before as f64 / page_count as f64 * 100.0,
                    opts.freelist_threshold * 100.0,
                );
            }

            let freelist_after: i64 =
                conn.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;

            Ok(CompactReport {
                wal_checkpointed: true,
                freelist_before,
                total_pages: page_count,
                freelist_after,
                vacuum_performed,
                message,
            })
        })
    }

    // ── Cleanup / clear methods ──

    /// Delete all checkpoints from storage.
    pub fn clear_all_checkpoints(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM time_index", [])?;
        let count2 = conn.execute("DELETE FROM checkpoints", [])?;
        Ok(count + count2)
    }

    /// Delete all branches from storage.
    pub fn clear_all_branches(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM branches", [])?;
        Ok(count as usize)
    }

    /// Delete all layers from storage.
    pub fn clear_all_layers(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM layers", [])?;
        Ok(count as usize)
    }

    /// Delete all snapshots from storage.
    pub fn clear_all_snapshots(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM snapshots", [])?;
        Ok(count as usize)
    }

    /// Delete all deltas from storage.
    pub fn clear_all_deltas(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let count = conn.execute("DELETE FROM deltas", [])?;
        Ok(count as usize)
    }

    /// Delete orphaned snapshots not referenced by any checkpoint.
    ///
    /// Uses SQLite's json_each to extract snapshot IDs stored as JSON arrays
    /// in the checkpoints table. Only snapshots whose IDs do not appear in
    /// any checkpoint's snapshot_ids are removed.
    pub fn cleanup_orphan_snapshots(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        // Delete snapshots whose ID is not referenced by any checkpoint's snapshot_ids JSON array
        let sql = "DELETE FROM snapshots WHERE id NOT IN (
            SELECT DISTINCT json_each.value
            FROM checkpoints, json_each(checkpoints.snapshot_ids)
        )";
        let count = conn.execute(sql, [])?;
        Ok(count as usize)
    }

    /// Delete orphaned deltas not referenced by any snapshot.
    ///
    /// Uses SQLite's json_each to extract delta IDs stored as JSON arrays
    /// in the snapshots table. Only deltas whose IDs do not appear in
    /// any snapshot's deltas array are removed.
    pub fn cleanup_orphan_deltas(&self) -> StorageResult<usize> {
        let conn = self.conn.lock();
        let sql = "DELETE FROM deltas WHERE id NOT IN (
            SELECT DISTINCT json_each.value
            FROM snapshots, json_each(snapshots.deltas)
        )";
        let count = conn.execute(sql, [])?;
        Ok(count as usize)
    }
}
