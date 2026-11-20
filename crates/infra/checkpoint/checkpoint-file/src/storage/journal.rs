//! Durable operation journal for managed `refs/wf/*` pointer changes.
//!
//! A record holds the intended compare-and-swap endpoints plus the final
//! value of every attached SQLite state. Git refs stay authoritative; this
//! table only records intent and supplies idempotent startup compensation.

use checkpoint_base::error::CheckpointError;

use super::connection::SqliteStorage;
use super::git_meta::{ReviewStatus, SourceIndexEntry};
use super::repository::StorageResult;

fn db_err(e: rusqlite::Error) -> CheckpointError {
    CheckpointError::Internal(format!("sqlite error: {e}"))
}

/// Kind of managed ref update, used for audit queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefOperationKind {
    FileCommit,
    EditGroupCommit,
    UndoEdit,
    RedoEdit,
    EditGroupRollback,
    ReviewSubmit,
    ReviewApprove,
    ReviewReject,
    ReviewMerge,
    FeatureMerge,
    FeatureRevert,
    ConflictResolve,
    ContextArchive,
    ExternalSync,
    GcRefCleanup,
}

impl RefOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FileCommit => "file_commit",
            Self::EditGroupCommit => "edit_group_commit",
            Self::UndoEdit => "undo_edit",
            Self::RedoEdit => "redo_edit",
            Self::EditGroupRollback => "edit_group_rollback",
            Self::ReviewSubmit => "review_submit",
            Self::ReviewApprove => "review_approve",
            Self::ReviewReject => "review_reject",
            Self::ReviewMerge => "review_merge",
            Self::FeatureMerge => "feature_merge",
            Self::FeatureRevert => "feature_revert",
            Self::ConflictResolve => "conflict_resolve",
            Self::ContextArchive => "context_archive",
            Self::ExternalSync => "external_sync",
            Self::GcRefCleanup => "gc_ref_cleanup",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "file_commit" => Self::FileCommit,
            "edit_group_commit" => Self::EditGroupCommit,
            "undo_edit" => Self::UndoEdit,
            "redo_edit" => Self::RedoEdit,
            "edit_group_rollback" => Self::EditGroupRollback,
            "review_submit" => Self::ReviewSubmit,
            "review_approve" => Self::ReviewApprove,
            "review_reject" => Self::ReviewReject,
            "review_merge" => Self::ReviewMerge,
            "feature_merge" => Self::FeatureMerge,
            "feature_revert" => Self::FeatureRevert,
            "conflict_resolve" => Self::ConflictResolve,
            "context_archive" => Self::ContextArchive,
            "external_sync" => Self::ExternalSync,
            "gc_ref_cleanup" => Self::GcRefCleanup,
            _ => return None,
        })
    }
}

/// Lifecycle of one journal record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefOperationStatus {
    Intent,
    Applied,
    Abandoned,
    Diverged,
}

impl RefOperationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intent => "intent",
            Self::Applied => "applied",
            Self::Abandoned => "abandoned",
            Self::Diverged => "diverged",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "intent" => Self::Intent,
            "applied" => Self::Applied,
            "abandoned" => Self::Abandoned,
            "diverged" => Self::Diverged,
            _ => return None,
        })
    }
}

/// Final redo-stack content for one actor. Snapshots are absolute values so
/// replaying an operation twice cannot append or pop twice.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JournalRedoStack {
    pub actor: String,
    pub entries: Vec<String>,
}

/// Final latest-checkpoint mirror entry (`None` removes the entry).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JournalLatestCheckpoint {
    pub actor: String,
    pub head: Option<String>,
}

/// Final review-state value (`None` removes the SQL row).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JournalReviewState {
    pub review_ref: String,
    pub status: Option<ReviewStatus>,
}

/// All SQLite state written after a successful ref CAS. Every field is a
/// final value, never an increment.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JournalEffects {
    pub redo_stack: Option<JournalRedoStack>,
    pub latest_checkpoint: Option<JournalLatestCheckpoint>,
    pub review_states: Vec<JournalReviewState>,
    pub deleted_review_refs: Vec<String>,
    pub source_index: Option<SourceIndexEntry>,
}

/// One persisted ref operation record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefOperationRecord {
    pub seq: i64,
    pub op_id: String,
    pub kind: RefOperationKind,
    pub reason: String,
    pub actor: String,
    pub session: String,
    pub tool: String,
    pub ref_name: String,
    /// CAS expected id; `None` means the ref must be absent.
    pub old_id: Option<String>,
    /// CAS target id; `None` means the ref is deleted.
    pub new_id: Option<String>,
    pub related_refs: Vec<String>,
    pub effects: JournalEffects,
    pub status: RefOperationStatus,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

fn encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec())
}

fn decode_related(bytes: &[u8]) -> Vec<String> {
    serde_json::from_slice(bytes).unwrap_or_default()
}

fn decode_effects(bytes: &[u8]) -> JournalEffects {
    serde_json::from_slice(bytes).unwrap_or_default()
}

fn decode_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RefOperationRecord> {
    let kind: String = row.get(2)?;
    let status: String = row.get(12)?;
    let related: Vec<u8> = row.get(9)?;
    let effects: Vec<u8> = row.get(10)?;
    Ok(RefOperationRecord {
        seq: row.get(0)?,
        op_id: row.get(1)?,
        kind: RefOperationKind::parse(&kind).unwrap_or(RefOperationKind::FileCommit),
        reason: row.get(3)?,
        actor: row.get(4)?,
        session: row.get(5)?,
        tool: row.get(6)?,
        ref_name: row.get(7)?,
        old_id: row.get(8)?,
        new_id: row.get(11)?,
        related_refs: decode_related(&related),
        effects: decode_effects(&effects),
        status: RefOperationStatus::parse(&status).unwrap_or(RefOperationStatus::Intent),
        created_at: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

const RECORD_COLUMNS: &str =
    "seq, op_id, op_type, reason, actor, session, tool, ref_name, old_id, related_refs,
     side_effects, new_id, status, created_at, completed_at";

impl SqliteStorage {
    /// Persist a fresh `intent` record before the ref CAS.
    pub fn insert_ref_operation(&self, record: &RefOperationRecord) -> StorageResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO ref_operations
                 (op_id, op_type, reason, actor, session, tool, ref_name, old_id,
                  new_id, related_refs, side_effects, status, created_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![
                    record.op_id,
                    record.kind.as_str(),
                    record.reason,
                    record.actor,
                    record.session,
                    record.tool,
                    record.ref_name,
                    record.old_id,
                    record.new_id,
                    encode(&record.related_refs),
                    encode(&record.effects),
                    RefOperationStatus::Intent.as_str(),
                    record.created_at,
                    Option::<i64>::None,
                ],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    /// Refresh the CAS endpoints of the same intent after a conflict: no
    /// extra speculative record is created.
    pub fn refresh_ref_operation(
        &self,
        op_id: &str,
        old_id: Option<&str>,
        new_id: Option<&str>,
    ) -> StorageResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE ref_operations SET old_id = ?2, new_id = ?3 WHERE op_id = ?1
                 AND status = 'intent'",
                rusqlite::params![op_id, old_id, new_id],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    /// Refresh CAS endpoints and attached state of the same intent.
    pub fn refresh_ref_operation_with(
        &self,
        op_id: &str,
        old_id: Option<&str>,
        new_id: Option<&str>,
        effects: Option<&JournalEffects>,
    ) -> StorageResult<()> {
        self.with_conn(|conn| {
            if let Some(effects) = effects {
                conn.execute(
                    "UPDATE ref_operations SET old_id = ?2, new_id = ?3, side_effects = ?4
                     WHERE op_id = ?1 AND status = 'intent'",
                    rusqlite::params![op_id, old_id, new_id, encode(effects)],
                )
                .map_err(db_err)?;
            } else {
                conn.execute(
                    "UPDATE ref_operations SET old_id = ?2, new_id = ?3
                     WHERE op_id = ?1 AND status = 'intent'",
                    rusqlite::params![op_id, old_id, new_id],
                )
                .map_err(db_err)?;
            }
            Ok(())
        })
    }

    /// Attached state of one record, used when completing an operation.
    pub fn load_ref_effects(&self, op_id: &str) -> StorageResult<JournalEffects> {
        self.with_conn(|conn| {
            let bytes: Vec<u8> = conn
                .query_row(
                    "SELECT side_effects FROM ref_operations WHERE op_id = ?1",
                    [op_id],
                    |row| row.get(0),
                )
                .map_err(db_err)?;
            Ok(decode_effects(&bytes))
        })
    }

    pub fn set_ref_operation_status(
        &self,
        op_id: &str,
        status: RefOperationStatus,
    ) -> StorageResult<()> {
        self.with_conn(|conn| {
            let completed_at = if status == RefOperationStatus::Applied {
                Some(chrono::Utc::now().timestamp_millis())
            } else {
                None
            };
            conn.execute(
                "UPDATE ref_operations SET status = ?2, completed_at = ?3 WHERE op_id = ?1",
                rusqlite::params![op_id, status.as_str(), completed_at],
            )
            .map_err(db_err)?;
            Ok(())
        })
    }

    pub fn list_ref_operations(&self) -> StorageResult<Vec<RefOperationRecord>> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {RECORD_COLUMNS} FROM ref_operations ORDER BY seq");
            let mut stmt = conn.prepare(&sql).map_err(db_err)?;
            let rows = stmt
                .query_map([], decode_row)
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            Ok(rows)
        })
    }

    pub fn list_ref_operations_for_ref(
        &self,
        refname: &str,
    ) -> StorageResult<Vec<RefOperationRecord>> {
        self.with_conn(|conn| {
            let sql = format!(
                "SELECT {RECORD_COLUMNS} FROM ref_operations WHERE ref_name = ?1 ORDER BY seq"
            );
            let mut stmt = conn.prepare(&sql).map_err(db_err)?;
            let rows = stmt
                .query_map([refname], decode_row)
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            Ok(rows)
        })
    }

    /// Delete only fully settled records older than `cutoff_ms` whose
    /// successor chain is also settled. Intent and diverged records stay.
    pub fn prune_ref_operations(&self, cutoff_ms: i64) -> StorageResult<usize> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(db_err)?;
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM ref_operations WHERE status = 'applied'
             AND completed_at IS NOT NULL AND completed_at < ?1 ORDER BY seq"
        );
        let candidates = {
            let mut stmt = tx.prepare(&sql).map_err(db_err)?;
            let rows = stmt
                .query_map([cutoff_ms], decode_row)
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?;
            rows
        };
        let mut removed = 0usize;
        for record in candidates {
            let unsettled_successor: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM ref_operations
                     WHERE ref_name = ?1 AND seq > ?2 AND status IN ('intent', 'diverged')",
                    rusqlite::params![record.ref_name, record.seq],
                    |row| row.get(0),
                )
                .map_err(db_err)?;
            if unsettled_successor > 0 {
                continue;
            }
            removed += tx
                .execute("DELETE FROM ref_operations WHERE seq = ?1", [record.seq])
                .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(
        kind: RefOperationKind,
        old_id: Option<&str>,
        new_id: Option<&str>,
    ) -> RefOperationRecord {
        RefOperationRecord {
            seq: 0,
            op_id: format!("op-{}", wf_common::generate_id()),
            kind,
            reason: "test".to_string(),
            actor: "agent:a".to_string(),
            session: String::new(),
            tool: "test".to_string(),
            ref_name: "refs/wf/edit/a".to_string(),
            old_id: old_id.map(str::to_string),
            new_id: new_id.map(str::to_string),
            related_refs: vec![],
            effects: JournalEffects::default(),
            status: RefOperationStatus::Intent,
            created_at: 1,
            completed_at: None,
        }
    }

    #[test]
    fn journal_records_intent_endpoints_and_settled_status() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        let mut entry = record(RefOperationKind::UndoEdit, Some("old"), Some("new"));
        storage.insert_ref_operation(&entry).unwrap();

        let stored = storage.list_ref_operations().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].old_id.as_deref(), Some("old"));
        assert_eq!(stored[0].status, RefOperationStatus::Intent);

        storage
            .refresh_ref_operation(&entry.op_id, Some("newer"), Some("newest"))
            .unwrap();
        storage
            .set_ref_operation_status(&entry.op_id, RefOperationStatus::Applied)
            .unwrap();
        entry.status = RefOperationStatus::Applied;
        let stored = storage.list_ref_operations().unwrap();
        assert_eq!(stored.len(), 1, "CAS retries reuse one record");
        assert_eq!(stored[0].old_id.as_deref(), Some("newer"));
        assert_eq!(stored[0].new_id.as_deref(), Some("newest"));
        assert_eq!(stored[0].status, RefOperationStatus::Applied);
        assert!(stored[0].completed_at.is_some());
    }

    #[test]
    fn journal_prune_keeps_unsettled_and_chained_records() {
        let storage = SqliteStorage::new_full_in_memory().unwrap();
        let old = record(RefOperationKind::FileCommit, None, Some("a"));
        storage.insert_ref_operation(&old).unwrap();
        storage
            .set_ref_operation_status(&old.op_id, RefOperationStatus::Applied)
            .unwrap();
        let pending = record(RefOperationKind::UndoEdit, Some("a"), Some("b"));
        storage.insert_ref_operation(&pending).unwrap();

        assert_eq!(storage.prune_ref_operations(i64::MAX).unwrap(), 0);
        storage
            .set_ref_operation_status(&pending.op_id, RefOperationStatus::Abandoned)
            .unwrap();
        assert_eq!(storage.prune_ref_operations(i64::MAX).unwrap(), 1);
    }
}
