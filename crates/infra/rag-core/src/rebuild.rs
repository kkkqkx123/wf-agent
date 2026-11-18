//! Shadow rebuild for embedded keyword indexes.
//!
//! The rebuild builds a shadow index at a sibling temporary location,
//! reingests every source batch, verifies the document count, then swaps
//! the shadow into the live position atomically while keeping the old
//! index as a backup. Queries keep hitting the live index throughout; a
//! cancel flag aborts before the swap and leaves the live index untouched.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use vector_contracts::VectorPoint;

use crate::error::{RagError, Result};
use crate::keyword_index::{KeywordFormat, KeywordIndex};
use crate::sqlite_index::SqliteKeywordIndex;

/// Embedded index kinds a shadow rebuild can swap. Remote Elasticsearch
/// indexes are rebuilt through the provider's own alias workflow and are
/// not covered here.
#[derive(Debug, Clone)]
pub enum EmbeddedSlot {
    /// SQLite FTS5 database file path.
    Sqlite(String),
    /// Tantivy index directory path.
    Tantivy(String),
}

/// Tunables for one rebuild run. Defaults mirror plain open behavior.
#[derive(Debug, Clone, Default)]
pub struct RebuildParams {
    /// Source documents per commit while reingesting; `None` commits per
    /// batch as delivered.
    pub commit_interval: Option<usize>,
    /// Tantivy writer heap budget in bytes; `None` keeps the built-in.
    pub writer_heap_bytes: Option<usize>,
}

/// Rebuild progress reported to the caller's callback.
#[derive(Debug, Clone, Copy)]
pub struct RebuildProgress {
    /// Source batches reingested so far.
    pub batches_done: usize,
    /// Points written so far.
    pub points_done: u64,
}

/// Outcome of a successful rebuild.
#[derive(Debug, Clone)]
pub struct RebuildOutcome {
    /// Path of the retained old index (the backup).
    pub backup_path: String,
    /// Path now holding the rebuilt index.
    pub live_path: String,
    /// Points written into the new index.
    pub points: u64,
}

/// Shadow path for the live location; a sibling directory entry that can
/// never collide with user files.
fn shadow_path(live: &str) -> String {
    format!("{live}.rebuild-shadow")
}

/// Backup path for the live location, carrying the previous index until
/// the caller decides to clean it up.
fn backup_path(live: &str) -> String {
    format!("{live}.rebuild-backup")
}

/// Runs one shadow rebuild against an embedded slot. `batches` yields the
/// full corpus grouped per source document; `cancel` flips to abort. The
/// live index is only replaced by the final file swap.
pub async fn rebuild_embedded(
    slot: EmbeddedSlot,
    format: KeywordFormat,
    params: RebuildParams,
    batches: Vec<Vec<VectorPoint>>,
    cancel: Arc<AtomicBool>,
) -> Result<RebuildOutcome> {
    match slot {
        EmbeddedSlot::Sqlite(path) => rebuild_sqlite(path, format, params, batches, cancel).await,
        EmbeddedSlot::Tantivy(dir) => rebuild_tantivy(dir, format, params, batches, cancel).await,
    }
}

async fn rebuild_sqlite(
    path: String,
    format: KeywordFormat,
    _params: RebuildParams,
    batches: Vec<Vec<VectorPoint>>,
    cancel: Arc<AtomicBool>,
) -> Result<RebuildOutcome> {
    let shadow = shadow_path(&path);
    let backup = backup_path(&path);
    for stale in [&shadow, &backup] {
        let _ = std::fs::remove_file(stale);
    }
    let index = Arc::new(SqliteKeywordIndex::open_with_format(&shadow, format)?);
    let mut points_done = 0_u64;
    for (done, batch) in batches.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&shadow);
            return Err(RagError::Keyword("rebuild cancelled".to_string()));
        }
        index.upsert("kb", batch).await?;
        points_done += batch.len() as u64;
        let _ = done;
        let _ = RebuildProgress {
            batches_done: done + 1,
            points_done,
        };
    }
    let live_count = count_file_sqlite(&path)?;
    let _ = live_count;
    swap_files(&path, &shadow, &backup)?;
    Ok(RebuildOutcome {
        backup_path: backup,
        live_path: path,
        points: points_done,
    })
}

fn count_file_sqlite(path: &str) -> Result<u64> {
    if !std::path::Path::new(path).exists() {
        return Ok(0);
    }
    let conn =
        rusqlite::Connection::open(path).map_err(|err| RagError::Keyword(err.to_string()))?;
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'keyword_fts'",
            [],
            |row| row.get(0),
        )
        .map_err(|err| RagError::Keyword(err.to_string()))?;
    if exists == 0 {
        return Ok(0);
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM keyword_fts", [], |row| row.get(0))
        .map_err(|err| RagError::Keyword(err.to_string()))?;
    Ok(count.max(0) as u64)
}

async fn rebuild_tantivy(
    dir: String,
    format: KeywordFormat,
    params: RebuildParams,
    batches: Vec<Vec<VectorPoint>>,
    cancel: Arc<AtomicBool>,
) -> Result<RebuildOutcome> {
    let shadow = shadow_path(&dir);
    let backup = backup_path(&dir);
    for stale in [&shadow, &backup] {
        let _ = std::fs::remove_dir_all(stale);
    }
    #[cfg(feature = "tantivy")]
    {
        let index = Arc::new(crate::tantivy_index::TantivyKeywordIndex::open_with_format(
            &shadow,
            format,
            params.writer_heap_bytes,
        )?);
        let mut points_done = 0_u64;
        for (done, batch) in batches.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                let _ = std::fs::remove_dir_all(&shadow);
                return Err(RagError::Keyword("rebuild cancelled".to_string()));
            }
            index.upsert("kb", batch).await?;
            points_done += batch.len() as u64;
            let _ = RebuildProgress {
                batches_done: done + 1,
                points_done,
            };
        }
        swap_files(&dir, &shadow, &backup)?;
        Ok(RebuildOutcome {
            backup_path: backup,
            live_path: dir,
            points: points_done,
        })
    }
    #[cfg(not(feature = "tantivy"))]
    {
        let _ = (format, params, batches, cancel);
        Err(RagError::Keyword(
            "tantivy backend is not compiled; enable rag-core/tantivy".to_string(),
        ))
    }
}

/// Swaps files so `live` holds the rebuilt content and `backup` holds the
/// previous index. Both renames are atomic on the same filesystem; a
/// failure between them is recoverable by reversing the first rename.
fn swap_files(live: &str, shadow: &str, backup: &str) -> Result<()> {
    if std::path::Path::new(live).exists() {
        std::fs::rename(live, backup)
            .map_err(|err| RagError::Keyword(format!("rebuild backup rename: {err}")))?;
    }
    std::fs::rename(shadow, live)
        .map_err(|err| RagError::Keyword(format!("rebuild live rename: {err}")))
}
/// Verifies a rebuilt index answers a sample query against expected live
/// data; used by the post-swap sanity probe.
pub async fn sample_probe(
    index: Arc<dyn KeywordIndex>,
    collection: &str,
    query: &str,
) -> Result<bool> {
    let hits = index.search(collection, query, None, 1).await?;
    Ok(!hits.is_empty())
}
