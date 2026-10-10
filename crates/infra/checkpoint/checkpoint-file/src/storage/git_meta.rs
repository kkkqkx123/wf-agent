use super::connection::SqliteStorage;
use super::repository::StorageResult;
use checkpoint_base::error::CheckpointError;

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
}

impl ReviewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ReviewStatus::Pending => "pending",
            ReviewStatus::Approved => "approved",
            ReviewStatus::Rejected => "rejected",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" => Some(ReviewStatus::Pending),
            "approved" => Some(ReviewStatus::Approved),
            "rejected" => Some(ReviewStatus::Rejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SourceIndexEntry {
    pub commit_id: String,
    pub actor: String,
    pub session: String,
    pub tool: String,
    pub paths: Vec<String>,
    pub timestamp: i64,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl SqliteStorage {
    pub fn set_review_state(&self, review_ref: &str, status: ReviewStatus) -> StorageResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO review_state (review_ref, status, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(review_ref) DO UPDATE SET status = excluded.status, updated_at = excluded.updated_at",
                rusqlite::params![review_ref, status.as_str(), now_millis()],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn get_review_state(&self, review_ref: &str) -> StorageResult<Option<ReviewStatus>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT status FROM review_state WHERE review_ref = ?1")
                .map_err(db_err)?;
            let mut rows = stmt.query([review_ref]).map_err(db_err)?;
            if let Some(row) = rows.next().map_err(db_err)? {
                let raw: String = row.get(0).map_err(db_err)?;
                Ok(ReviewStatus::parse(&raw))
            } else {
                Ok(None)
            }
        })
    }

    pub fn delete_review_state(&self, review_ref: &str) -> StorageResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "DELETE FROM review_state WHERE review_ref = ?1",
                [review_ref],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn list_review_states(&self) -> StorageResult<Vec<(String, ReviewStatus)>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT review_ref, status FROM review_state ORDER BY updated_at DESC")
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |row| {
                    let name: String = row.get(0)?;
                    let raw: String = row.get(1)?;
                    Ok((name, raw))
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            Ok(rows
                .into_iter()
                .filter_map(|(name, raw)| ReviewStatus::parse(&raw).map(|s| (name, s)))
                .collect())
        })
    }

    pub fn store_empty_dirs(&self, commit_id: &str, dirs: &[String]) -> StorageResult<()> {
        let payload = serde_json::to_vec(dirs).unwrap_or_default();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO empty_dirs (commit_id, dirs, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(commit_id) DO UPDATE SET dirs = excluded.dirs, updated_at = excluded.updated_at",
                rusqlite::params![commit_id, payload, now_millis()],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn load_empty_dirs(&self, commit_id: &str) -> StorageResult<Vec<String>> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT dirs FROM empty_dirs WHERE commit_id = ?1")
                .map_err(db_err)?;
            let mut rows = stmt.query([commit_id]).map_err(db_err)?;
            if let Some(row) = rows.next().map_err(db_err)? {
                let payload: Vec<u8> = row.get(0).map_err(db_err)?;
                Ok(serde_json::from_slice(&payload).unwrap_or_default())
            } else {
                Ok(Vec::new())
            }
        })
    }

    /// Single-level transaction owned by this call: the transaction object
    /// is the critical section, so nested manual begin/commit strings can no
    /// longer mis-pair `COMMIT`/`ROLLBACK` on the shared connection.
    pub fn record_source_index(&self, entry: &SourceIndexEntry) -> StorageResult<()> {
        let payload = serde_json::to_vec(&entry.paths).unwrap_or_default();
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO source_index (commit_id, actor, session, tool, paths, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(commit_id) DO UPDATE SET actor = excluded.actor, session = excluded.session,
                tool = excluded.tool, paths = excluded.paths, timestamp = excluded.timestamp",
            rusqlite::params![
                entry.commit_id,
                entry.actor,
                entry.session,
                entry.tool,
                payload,
                entry.timestamp
            ],
        )
        .map_err(db_err)?;
        tx.execute(
            "DELETE FROM source_index_paths WHERE commit_id = ?1",
            [&entry.commit_id],
        )
        .map_err(db_err)?;
        for path in &entry.paths {
            tx.execute(
                "INSERT OR IGNORE INTO source_index_paths (commit_id, path) VALUES (?1, ?2)",
                rusqlite::params![entry.commit_id, path],
            )
            .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    pub fn find_commits_by_actor(
        &self,
        actor: &str,
        limit: usize,
    ) -> StorageResult<Vec<SourceIndexEntry>> {
        self.with_conn(|conn| {
            let sql = if limit > 0 {
                "SELECT commit_id, actor, session, tool, paths, timestamp
                 FROM source_index WHERE actor = ?1 ORDER BY timestamp DESC LIMIT ?2"
            } else {
                "SELECT commit_id, actor, session, tool, paths, timestamp
                 FROM source_index WHERE actor = ?1 ORDER BY timestamp DESC"
            };
            let mut stmt = conn.prepare(sql).map_err(db_err)?;
            let rows = if limit > 0 {
                stmt.query_map(rusqlite::params![actor, limit as i64], decode_source_row)
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?
            } else {
                stmt.query_map([actor], decode_source_row)
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?
            };
            Ok(rows)
        })
    }

    pub fn find_commits_by_path(
        &self,
        path: &str,
        limit: usize,
    ) -> StorageResult<Vec<SourceIndexEntry>> {
        self.with_conn(|conn| {
            let sql = if limit > 0 {
                "SELECT s.commit_id, s.actor, s.session, s.tool, s.paths, s.timestamp
                 FROM source_index s JOIN source_index_paths p ON p.commit_id = s.commit_id
                 WHERE p.path = ?1 ORDER BY s.timestamp DESC LIMIT ?2"
            } else {
                "SELECT s.commit_id, s.actor, s.session, s.tool, s.paths, s.timestamp
                 FROM source_index s JOIN source_index_paths p ON p.commit_id = s.commit_id
                 WHERE p.path = ?1 ORDER BY s.timestamp DESC"
            };
            let mut stmt = conn.prepare(sql).map_err(db_err)?;
            let rows = if limit > 0 {
                stmt.query_map(rusqlite::params![path, limit as i64], decode_source_row)
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?
            } else {
                stmt.query_map([path], decode_source_row)
                    .map_err(db_err)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(db_err)?
            };
            Ok(rows)
        })
    }

    pub fn clear_source_index(&self) -> StorageResult<usize> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute("DELETE FROM source_index_paths", [])
            .map_err(db_err)?;
        let count = tx.execute("DELETE FROM source_index", []).map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(count)
    }

    pub fn delete_source_index(&self, commit_id: &str) -> StorageResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "DELETE FROM source_index_paths WHERE commit_id = ?1",
            [commit_id],
        )
        .map_err(db_err)?;
        tx.execute("DELETE FROM source_index WHERE commit_id = ?1", [commit_id])
            .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(())
    }

    /// Record which file commit a state checkpoint was projected from, so
    /// state restore can resolve the exact file set instead of "latest".
    /// Stored in the metadata key-value area; the object store stays the
    /// only source of file bytes.
    pub fn record_state_file_link(
        &self,
        state_checkpoint_id: &str,
        file_commit_id: &str,
    ) -> StorageResult<()> {
        let key = format!("{STATE_FILE_LINK_PREFIX}{state_checkpoint_id}");
        self.with_conn(|conn| {
            conn.execute(
                "INSERT OR REPLACE INTO meta_kv (key, value, updated_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![key, file_commit_id.as_bytes(), now_millis()],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    /// Look up the file commit linked to a state checkpoint, if any.
    /// Links predate nothing: checkpoints created before linking simply
    /// have no entry.
    pub fn lookup_state_file_link(
        &self,
        state_checkpoint_id: &str,
    ) -> StorageResult<Option<String>> {
        let key = format!("{STATE_FILE_LINK_PREFIX}{state_checkpoint_id}");
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT value FROM meta_kv WHERE key = ?1")
                .map_err(db_err)?;
            let result = stmt.query_row(rusqlite::params![key], |row| {
                let value: Vec<u8> = row.get(0)?;
                String::from_utf8(value)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
            });
            match result {
                Ok(value) => Ok(Some(value)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(db_err(e)),
            }
        })
    }
}

/// Key prefix for state-checkpoint to file-commit links in `meta_kv`.
const STATE_FILE_LINK_PREFIX: &str = "state_file_link:";

fn decode_source_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceIndexEntry> {
    let payload: Vec<u8> = row.get(4)?;
    Ok(SourceIndexEntry {
        commit_id: row.get(0)?,
        actor: row.get(1)?,
        session: row.get(2)?,
        tool: row.get(3)?,
        paths: serde_json::from_slice(&payload).unwrap_or_default(),
        timestamp: row.get(5)?,
    })
}
