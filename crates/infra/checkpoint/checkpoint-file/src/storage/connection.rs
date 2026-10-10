use parking_lot::Mutex;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::repository::StorageResult;
use checkpoint_base::error::CheckpointError;

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

#[derive(Debug, Clone)]
pub struct CompactOptions {
    pub freelist_threshold: f64,
    pub max_vacuum_pages: i64,
    pub vacuum_full: bool,
}

impl Default for CompactOptions {
    fn default() -> Self {
        CompactOptions {
            freelist_threshold: 0.10,
            max_vacuum_pages: 1000,
            vacuum_full: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompactReport {
    pub wal_checkpointed: bool,
    pub freelist_before: i64,
    pub total_pages: i64,
    pub freelist_after: i64,
    pub vacuum_performed: bool,
    pub message: String,
}

pub struct SqliteStorage {
    pub conn: Arc<Mutex<Connection>>,
    path: Option<PathBuf>,
}

impl Clone for SqliteStorage {
    fn clone(&self) -> Self {
        SqliteStorage {
            conn: self.conn.clone(),
            path: self.path.clone(),
        }
    }
}

impl SqliteStorage {
    pub fn new_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        super::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    pub fn new_full_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        super::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(Mutex::new(conn)),
            path: None,
        })
    }

    pub fn new(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path).map_err(db_err)?;
        super::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn new_full(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path).map_err(db_err)?;
        super::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(Mutex::new(conn)),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn new_with_connection_arc(conn: &Arc<Mutex<Connection>>) -> Self {
        SqliteStorage {
            conn: conn.clone(),
            path: None,
        }
    }

    pub fn share(&self) -> Self {
        self.clone()
    }

    pub fn db_path(&self) -> Option<std::path::PathBuf> {
        self.path.clone()
    }

    /// Single-level locking: the closure must not call back into storage
    /// methods that lock again. A plain mutex backs the connection so
    /// nested use deadlocks loudly instead of mis-pairing transactions.
    pub fn with_conn<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Connection) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        f(&conn)
    }

    pub fn run_maintenance(&self) -> StorageResult<CompactReport> {
        self.run_maintenance_with(&CompactOptions::default())
    }

    /// Maintenance runs on its own connection so a long `VACUUM` never holds
    /// the shared write lock: writers keep flowing and only coordinate with
    /// SQLite file locks (with the same busy timeout). In-memory stores have
    /// no file to reopen and fall back to the shared connection.
    pub fn run_maintenance_with(&self, opts: &CompactOptions) -> StorageResult<CompactReport> {
        match self.path.clone() {
            Some(path) => {
                let conn = Connection::open(&path).map_err(db_err)?;
                conn.execute_batch("PRAGMA busy_timeout = 5000;")
                    .map_err(db_err)?;
                compact_on(&conn, opts)
            }
            None => self.with_conn(|conn| compact_on(conn, opts)),
        }
    }
}

fn compact_on(conn: &Connection, opts: &CompactOptions) -> StorageResult<CompactReport> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(db_err)?;

    let page_count: i64 = conn
        .query_row("PRAGMA page_count", [], |row| row.get(0))
        .map_err(db_err)?;
    let freelist_before: i64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(db_err)?;

    let (vacuum_performed, message) = if page_count == 0 {
        (false, "database is empty, nothing to compact".into())
    } else if opts.vacuum_full {
        conn.execute_batch("VACUUM;").map_err(db_err)?;
        (true, "full VACUUM completed".into())
    } else if freelist_before as f64 / page_count as f64 > opts.freelist_threshold {
        let pages = freelist_before.min(opts.max_vacuum_pages);
        conn.execute(&format!("PRAGMA incremental_vacuum({})", pages), [])
            .map_err(db_err)?;
        (
            true,
            format!(
                "incremental_vacuum: reclaimed up to {} pages (freelist was {}/{})",
                pages, freelist_before, page_count
            ),
        )
    } else {
        (
            false,
            format!(
                "freelist ratio {:.2}% below threshold {:.0}%, skipped vacuum",
                freelist_before as f64 / page_count as f64 * 100.0,
                opts.freelist_threshold * 100.0,
            ),
        )
    };

    let freelist_after: i64 = conn
        .query_row("PRAGMA freelist_count", [], |row| row.get(0))
        .map_err(db_err)?;

    Ok(CompactReport {
        wal_checkpointed: true,
        freelist_before,
        total_pages: page_count,
        freelist_after,
        vacuum_performed,
        message,
    })
}
