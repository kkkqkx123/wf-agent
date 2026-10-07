use parking_lot::ReentrantMutex;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;

use super::repository::{AtomicOps, Repository, StorageResult};
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
    pub conn: Arc<ReentrantMutex<Connection>>,
}

impl Clone for SqliteStorage {
    fn clone(&self) -> Self {
        SqliteStorage {
            conn: self.conn.clone(),
        }
    }
}

impl AtomicOps for SqliteStorage {
    fn with_atomic<F, T>(&self, f: F) -> StorageResult<T>
    where
        F: FnOnce(&Self) -> StorageResult<T>,
    {
        let conn = self.conn.lock();
        conn.execute_batch("SAVEPOINT atomic_savepoint;")
            .map_err(db_err)?;
        match f(self) {
            Ok(value) => {
                conn.execute_batch("RELEASE SAVEPOINT atomic_savepoint;")
                    .map_err(db_err)?;
                drop(conn);
                Ok(value)
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK TO SAVEPOINT atomic_savepoint;")
                    .map_err(db_err)?;
                drop(conn);
                Err(e)
            }
        }
    }
}

impl<T: AtomicOps> Repository for T {}

impl SqliteStorage {
    pub fn new_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        super::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_full_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        super::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path).map_err(db_err)?;
        super::migrations::initialize_database(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_full(path: &Path) -> StorageResult<Self> {
        let conn = Connection::open(path).map_err(db_err)?;
        super::migrations::initialize_full(&conn)?;
        Ok(SqliteStorage {
            conn: Arc::new(ReentrantMutex::new(conn)),
        })
    }

    pub fn new_with_connection_arc(conn: &Arc<ReentrantMutex<Connection>>) -> Self {
        SqliteStorage { conn: conn.clone() }
    }

    pub fn list_metadata_by_prefix(&self, prefix: &str) -> StorageResult<Vec<(String, String)>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT key, value FROM meta_kv WHERE key LIKE ?1 ORDER BY key")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([format!("{}%", prefix)], |row| {
                    let key: String = row.get(0)?;
                    let value: Vec<u8> = row.get(1)?;
                    let value = String::from_utf8(value)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    Ok((key, value))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            Ok(rows)
        })
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

    pub fn run_maintenance(&self) -> StorageResult<CompactReport> {
        self.run_maintenance_with(&CompactOptions::default())
    }

    pub fn run_maintenance_with(&self, opts: &CompactOptions) -> StorageResult<CompactReport> {
        self.with_conn(|conn| {
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
        })
    }
}
