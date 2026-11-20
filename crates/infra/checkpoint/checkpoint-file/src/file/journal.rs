//! Ref-update journal protocol and startup reconciliation.
//!
//! Every managed ref change writes a final-valued `intent` record before its
//! CAS. Attached SQLite state is written after the CAS and before the record
//! is marked `applied`, so startup can finish any interrupted step without
//! guessing at the authoritative Git ref.

use std::collections::BTreeMap;

use crate::event::CheckpointEventBus;
use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
use crate::git_store::{CommitOnRefOutcome, GitStoreError, MAX_COMMIT_CAS_RETRIES};
use crate::storage::{
    JournalEffects, JournalLatestCheckpoint, JournalRedoStack, JournalReviewState,
    RefOperationKind, RefOperationRecord, RefOperationStatus, SourceIndexEntry, SqliteStorage,
};
use checkpoint_base::error::CheckpointError;

/// One managed ref update to record around a CAS.
#[derive(Debug, Clone)]
pub(crate) struct RefUpdateSpec {
    pub kind: RefOperationKind,
    pub reason: String,
    pub actor: String,
    pub session: String,
    pub tool: String,
    pub ref_name: String,
    pub old_id: Option<String>,
    pub new_id: Option<String>,
    pub related_refs: Vec<String>,
    pub effects: JournalEffects,
}

/// Handle for the single `intent` record of one ref update.
pub(crate) struct RefUpdateIntent {
    storage: SqliteStorage,
    op_id: String,
    effects: JournalEffects,
}

impl RefUpdateIntent {
    /// Point the same record at a fresh CAS attempt after a conflict.
    pub(crate) fn refresh(
        &self,
        old_id: Option<&str>,
        new_id: Option<&str>,
        effects: Option<&JournalEffects>,
    ) -> Result<(), CheckpointError> {
        self.storage
            .refresh_ref_operation_with(&self.op_id, old_id, new_id, effects)
    }

    /// Write attached state first, then mark the operation applied.
    pub(crate) fn complete(self, manager: &FileCheckpointManager) -> Result<(), CheckpointError> {
        manager.apply_journal_effects(&self.effects)?;
        self.storage
            .set_ref_operation_status(&self.op_id, RefOperationStatus::Applied)
    }

    pub(crate) fn abandon(self) -> Result<(), CheckpointError> {
        self.storage
            .set_ref_operation_status(&self.op_id, RefOperationStatus::Abandoned)
    }
}

/// Delete-state and index inputs for a journaled commit-on-ref call.
#[derive(Debug, Clone)]
pub(crate) struct JournalCommitSpec {
    pub kind: RefOperationKind,
    pub reason: String,
    pub actor: String,
    pub session: String,
    pub tool: String,
    /// `(actor, session, tool, paths)` for the new commit's source index.
    pub source_index: Option<(String, String, String, Vec<String>)>,
    /// Actor whose redo stack must be cleared by this new commit.
    pub clear_redo_actor: Option<String>,
    /// Actor whose latest-checkpoint mirror follows the new head.
    pub latest_actor: Option<String>,
    pub related_refs: Vec<String>,
}

/// Outcome of startup reconciliation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefReconcileReport {
    pub applied: usize,
    pub abandoned: usize,
    pub diverged: usize,
}

impl FileCheckpointManager {
    /// Begin a ref operation by persisting its `intent` record.
    pub(crate) fn begin_ref_operation(
        &self,
        spec: &RefUpdateSpec,
    ) -> Result<RefUpdateIntent, CheckpointError> {
        let storage = self.storage_ref()?;
        let op_id = wf_common::generate_id();
        storage.insert_ref_operation(&RefOperationRecord {
            seq: 0,
            op_id: op_id.clone(),
            kind: spec.kind,
            reason: spec.reason.clone(),
            actor: spec.actor.clone(),
            session: spec.session.clone(),
            tool: spec.tool.clone(),
            ref_name: spec.ref_name.clone(),
            old_id: spec.old_id.clone(),
            new_id: spec.new_id.clone(),
            related_refs: spec.related_refs.clone(),
            effects: spec.effects.clone(),
            status: RefOperationStatus::Intent,
            created_at: self.creation_timestamp()?,
            completed_at: None,
        })?;
        Ok(RefUpdateIntent {
            storage: storage.share(),
            op_id,
            effects: spec.effects.clone(),
        })
    }

    /// Run one managed ref CAS under the journal protocol. The callback may
    /// revise the expected/new endpoints and attached state on each retry;
    /// the same intent record is refreshed instead of being duplicated.
    pub(crate) fn journaled_ref_update<F>(
        &self,
        mut spec: RefUpdateSpec,
        mut cas: F,
    ) -> Result<(), CheckpointError>
    where
        F: FnMut(&mut RefUpdateSpec) -> Result<(), GitStoreError>,
    {
        let intent = self.begin_ref_operation(&spec)?;
        for _ in 0..MAX_COMMIT_CAS_RETRIES {
            match cas(&mut spec) {
                Ok(()) => {
                    intent.refresh(
                        spec.old_id.as_deref(),
                        spec.new_id.as_deref(),
                        Some(&spec.effects),
                    )?;
                    return intent.complete(self);
                }
                Err(GitStoreError::RefConflict(_)) => {
                    let observed = self
                        .git_ref()?
                        .read_ref(&spec.ref_name)
                        .map_err(map_git_error)?;
                    spec.old_id = observed;
                    intent.refresh(
                        spec.old_id.as_deref(),
                        spec.new_id.as_deref(),
                        Some(&spec.effects),
                    )?;
                }
                Err(other) => {
                    intent.abandon()?;
                    return Err(map_git_error(other));
                }
            }
        }
        intent.abandon()?;
        Err(CheckpointError::Branch(format!(
            "concurrent ref update conflict on '{}'",
            spec.ref_name
        )))
    }

    /// Append a commit on a managed ref under the journal protocol. Objects
    /// are written before the ref CAS; the record carries final-valued
    /// attached state computed from the exact commit the CAS will install.
    pub(crate) fn commit_on_ref_journaled(
        &self,
        refname: &str,
        changes: &std::collections::HashMap<String, Option<(String, Vec<u8>)>>,
        author: &str,
        message: &str,
        spec: JournalCommitSpec,
    ) -> Result<CommitOnRefOutcome, CheckpointError> {
        let git = self.git_ref()?;
        let mut intent: Option<RefUpdateIntent> = None;
        for _ in 0..MAX_COMMIT_CAS_RETRIES {
            let head = git.read_ref(refname).map_err(map_git_error)?;
            let parent_tree = match &head {
                Some(id) => Some(git.read_commit(id).map_err(map_git_error)?.tree),
                None => None,
            };
            let tree = git
                .build_tree_from_parent(parent_tree.as_deref(), changes)
                .map_err(map_git_error)?;
            if parent_tree.as_deref() == Some(tree.as_str()) {
                if let Some(intent) = intent {
                    intent.abandon()?;
                }
                return Ok(CommitOnRefOutcome {
                    id: head.unwrap_or_default(),
                    created: false,
                });
            }
            let parents = head.clone().into_iter().collect::<Vec<_>>();
            let id = git
                .write_commit(
                    &tree,
                    &parents,
                    author,
                    crate::git_store::SYSTEM_COMMITTER,
                    self.creation_timestamp()?,
                    message,
                )
                .map_err(map_git_error)?;
            let effects = journal_effects_for_commit(&spec, &id, git);
            let update = RefUpdateSpec {
                kind: spec.kind,
                reason: spec.reason.clone(),
                actor: spec.actor.clone(),
                session: spec.session.clone(),
                tool: spec.tool.clone(),
                ref_name: refname.to_string(),
                old_id: head.clone(),
                new_id: Some(id.clone()),
                related_refs: spec.related_refs.clone(),
                effects: effects.clone(),
            };
            match intent.as_mut() {
                Some(current) => {
                    current.effects = effects.clone();
                    current.refresh(head.as_deref(), Some(&id), Some(&effects))?;
                }
                None => intent = Some(self.begin_ref_operation(&update)?),
            }
            match git.compare_and_swap(refname, head.as_deref(), &id) {
                Ok(()) => {
                    intent.expect("intent started before CAS").complete(self)?;
                    return Ok(CommitOnRefOutcome { id, created: true });
                }
                Err(GitStoreError::RefConflict(_)) => continue,
                Err(other) => {
                    intent.expect("intent started before CAS").abandon()?;
                    return Err(map_git_error(other));
                }
            }
        }
        if let Some(intent) = intent {
            intent.abandon()?;
        }
        Err(CheckpointError::Branch(format!(
            "concurrent ref update conflict on '{refname}'"
        )))
    }

    /// Apply the final values carried by one record. Idempotent by design:
    /// every write replaces state with a complete snapshot.
    pub(crate) fn apply_journal_effects(
        &self,
        effects: &JournalEffects,
    ) -> Result<(), CheckpointError> {
        let storage = self.storage_ref()?;
        for review in &effects.review_states {
            match review.status {
                Some(status) => storage.set_review_state(&review.review_ref, status)?,
                None => storage.delete_review_state(&review.review_ref)?,
            }
        }
        for refname in &effects.deleted_review_refs {
            if let Err(err) = self.git_ref()?.delete_ref(refname) {
                if !matches!(err, GitStoreError::RefNotFound(_)) {
                    return Err(map_git_error(err));
                }
            }
        }
        if let Some(index) = &effects.source_index {
            storage.record_source_index(index)?;
        }
        if let Some(redo) = &effects.redo_stack {
            self.redo_stacks.set_snapshot(&redo.actor, &redo.entries)?;
        }
        if let Some(latest) = &effects.latest_checkpoint {
            match &latest.head {
                Some(head) => {
                    self.store
                        .latest_checkpoints
                        .insert(latest.actor.clone(), head.clone());
                }
                None => {
                    self.store.latest_checkpoints.remove(&latest.actor);
                }
            }
        }
        Ok(())
    }

    /// Reconcile interrupted ref operations against the authoritative refs.
    /// Only `intent` records are touched; settled records are never rewritten.
    pub fn reconcile_ref_operations(&self) -> Result<RefReconcileReport, CheckpointError> {
        let storage = self.storage_ref()?;
        let git = self.git_ref()?;
        let mut report = RefReconcileReport::default();
        let mut by_ref: BTreeMap<String, Vec<RefOperationRecord>> = BTreeMap::new();
        for record in storage.list_ref_operations()? {
            by_ref
                .entry(record.ref_name.clone())
                .or_default()
                .push(record);
        }
        let mut replay: Vec<RefOperationRecord> = Vec::new();
        for (refname, records) in by_ref {
            let current = git.read_ref(&refname).map_err(map_git_error)?;
            let mut resolved: Vec<Option<RefOperationStatus>> = vec![None; records.len()];
            for index in (0..records.len()).rev() {
                if records[index].status != RefOperationStatus::Intent {
                    resolved[index] = Some(records[index].status);
                    continue;
                }
                let record = &records[index];
                let chained = records[index + 1..]
                    .iter()
                    .enumerate()
                    .any(|(offset, next)| {
                        next.old_id == record.new_id
                            && resolved[index + 1 + offset] == Some(RefOperationStatus::Applied)
                    });
                let outcome = if current == record.new_id || chained {
                    RefOperationStatus::Applied
                } else if current == record.old_id {
                    RefOperationStatus::Abandoned
                } else {
                    RefOperationStatus::Diverged
                };
                resolved[index] = Some(outcome);
            }
            for (index, record) in records.iter().enumerate() {
                let Some(outcome) = resolved[index] else {
                    continue;
                };
                if record.status == RefOperationStatus::Intent && outcome != record.status {
                    storage.set_ref_operation_status(&record.op_id, outcome)?;
                    match outcome {
                        RefOperationStatus::Applied => {
                            replay.push(record.clone());
                            report.applied += 1;
                        }
                        RefOperationStatus::Abandoned => report.abandoned += 1,
                        RefOperationStatus::Diverged => {
                            report.diverged += 1;
                            if let Some(ref bus) = self.event_bus {
                                bus.publish(CheckpointEventBus::failed(
                                    "ref_operation_reconcile",
                                    format!(
                                        "ref '{refname}' operation '{}' diverged from journal chain",
                                        record.op_id
                                    ),
                                ));
                            }
                        }
                        RefOperationStatus::Intent => {}
                    }
                }
            }
        }
        replay.sort_by_key(|record| record.seq);
        for record in replay {
            self.apply_journal_effects(&record.effects)?;
        }
        Ok(report)
    }

    /// Audit view of every recorded ref pointer change, ordered by sequence.
    pub fn list_ref_operations(&self) -> Result<Vec<RefOperationRecord>, CheckpointError> {
        self.storage_ref()?.list_ref_operations()
    }

    /// Audit view of one ref's pointer history, ordered by sequence.
    pub fn list_ref_operations_for_ref(
        &self,
        refname: &str,
    ) -> Result<Vec<RefOperationRecord>, CheckpointError> {
        self.storage_ref()?.list_ref_operations_for_ref(refname)
    }

    /// Compress settled journal rows together with object-store GC.
    pub(crate) fn compact_ref_operations(
        &self,
        retention_ms: i64,
    ) -> Result<usize, CheckpointError> {
        self.storage_ref()?.prune_ref_operations(retention_ms)
    }
}

fn journal_effects_for_commit(
    spec: &JournalCommitSpec,
    commit_id: &str,
    git: &crate::git_store::GitStore,
) -> JournalEffects {
    let timestamp = git
        .read_commit(commit_id)
        .map(|commit| commit.committer_ts)
        .unwrap_or(0);
    JournalEffects {
        source_index: spec
            .source_index
            .as_ref()
            .map(|(actor, session, tool, paths)| SourceIndexEntry {
                commit_id: commit_id.to_string(),
                actor: actor.clone(),
                session: session.clone(),
                tool: tool.clone(),
                paths: paths.clone(),
                timestamp,
            }),
        redo_stack: spec
            .clear_redo_actor
            .as_ref()
            .map(|actor| JournalRedoStack {
                actor: actor.clone(),
                entries: Vec::new(),
            }),
        latest_checkpoint: spec
            .latest_actor
            .as_ref()
            .map(|actor| JournalLatestCheckpoint {
                actor: actor.clone(),
                head: Some(commit_id.to_string()),
            }),
        ..JournalEffects::default()
    }
}

/// Construct the effects of a redo-stack snapshot update.
pub(crate) fn redo_effects(actor: &str, entries: Vec<String>) -> JournalEffects {
    JournalEffects {
        redo_stack: Some(JournalRedoStack {
            actor: actor.to_string(),
            entries,
        }),
        latest_checkpoint: Some(JournalLatestCheckpoint {
            actor: actor.to_string(),
            head: None,
        }),
        ..JournalEffects::default()
    }
}

/// Construct the effects of a redo snapshot plus an explicit new head.
pub(crate) fn redo_effects_with_head(
    actor: &str,
    entries: Vec<String>,
    head: Option<String>,
) -> JournalEffects {
    JournalEffects {
        redo_stack: Some(JournalRedoStack {
            actor: actor.to_string(),
            entries,
        }),
        latest_checkpoint: Some(JournalLatestCheckpoint {
            actor: actor.to_string(),
            head,
        }),
        ..JournalEffects::default()
    }
}

/// Construct the effects of a review-state transition after a ref deletion.
pub(crate) fn review_effects(
    review_ref: &str,
    status: Option<crate::storage::ReviewStatus>,
) -> JournalEffects {
    JournalEffects {
        review_states: vec![JournalReviewState {
            review_ref: review_ref.to_string(),
            status,
        }],
        ..JournalEffects::default()
    }
}

/// Construct source-index effects for an already-written commit.
pub(crate) fn index_effects(entry: SourceIndexEntry) -> JournalEffects {
    JournalEffects {
        source_index: Some(entry),
        ..JournalEffects::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_splits_unlanded_landed_and_diverged_records() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let git = manager.git_ref().unwrap();
        let tree = git
            .build_tree_from_parent(None, &Default::default())
            .unwrap();
        let a = git
            .write_commit(&tree, &[], "a", crate::git_store::SYSTEM_COMMITTER, 1, "a")
            .unwrap();
        let b = git
            .write_commit(
                &tree,
                &[a.clone()],
                "a",
                crate::git_store::SYSTEM_COMMITTER,
                2,
                "b",
            )
            .unwrap();
        let refname = "refs/wf/feat/reconcile";
        git.write_ref(refname, &b).unwrap();
        let storage = manager.storage_ref().unwrap();
        for (old, new) in [
            (None, Some(a.clone())),
            (Some(a.clone()), Some(b.clone())),
            (Some(b.clone()), Some("dead".to_string())),
            (Some("dead".to_string()), Some("beef".to_string())),
        ] {
            storage
                .insert_ref_operation(&RefOperationRecord {
                    seq: 0,
                    op_id: wf_common::generate_id(),
                    kind: RefOperationKind::FeatureMerge,
                    reason: String::new(),
                    actor: String::new(),
                    session: String::new(),
                    tool: String::new(),
                    ref_name: refname.to_string(),
                    old_id: old,
                    new_id: new,
                    related_refs: vec![],
                    effects: JournalEffects::default(),
                    status: RefOperationStatus::Intent,
                    created_at: 1,
                    completed_at: None,
                })
                .unwrap();
        }
        let report = manager.reconcile_ref_operations().unwrap();
        assert_eq!(report.applied, 2, "first two records form the landed chain");
        assert_eq!(report.abandoned, 1);
        assert_eq!(report.diverged, 1);
        let records = storage.list_ref_operations_for_ref(refname).unwrap();
        assert_eq!(records[0].status, RefOperationStatus::Applied);
        assert_eq!(records[1].status, RefOperationStatus::Applied);
        assert_eq!(records[2].status, RefOperationStatus::Abandoned);
        assert_eq!(records[3].status, RefOperationStatus::Diverged);
        assert_eq!(git.read_ref(refname).unwrap().as_deref(), Some(b.as_str()));
    }

    #[test]
    fn reconcile_replays_final_redo_snapshot_once() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        let head = manager.apply_agent_edit(&actor, "a.txt", b"v2\n").unwrap();
        manager.undo_edit("entity-1").unwrap();
        let records = manager.list_ref_operations().unwrap();
        let undo = records
            .iter()
            .find(|record| record.kind == RefOperationKind::UndoEdit)
            .expect("undo journal record");
        assert_eq!(undo.status, RefOperationStatus::Applied);
        assert_eq!(
            undo.effects.redo_stack.as_ref().map(|r| r.entries.len()),
            Some(1)
        );

        // Simulate a crash after the ref CAS but before the redo write.
        manager
            .storage_ref()
            .unwrap()
            .set_ref_operation_status(&undo.op_id, RefOperationStatus::Intent)
            .unwrap();
        manager
            .redo_stacks
            .set_snapshot(actor.as_str(), &[])
            .unwrap();
        assert!(!manager.can_redo("entity-1").unwrap());

        let report = manager.reconcile_ref_operations().unwrap();
        assert_eq!(report.applied, 1);
        assert_eq!(
            manager.redo_stacks.last(actor.as_str()).as_deref(),
            Some(head.as_str())
        );
        assert_eq!(manager.reconcile_ref_operations().unwrap().applied, 0);
    }

    #[test]
    fn reconcile_abandons_intent_before_cas() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let actor = manager.actor_id_for("entity-1");
        let head = manager.apply_agent_edit(&actor, "a.txt", b"v1\n").unwrap();
        let record = RefOperationRecord {
            seq: 0,
            op_id: wf_common::generate_id(),
            kind: RefOperationKind::UndoEdit,
            reason: String::new(),
            actor: actor.as_str().to_string(),
            session: String::new(),
            tool: String::new(),
            ref_name: crate::git_store::edit_ref_for_actor(actor.as_str()),
            old_id: Some(head.clone()),
            new_id: Some("dead".to_string()),
            related_refs: vec![],
            effects: JournalEffects::default(),
            status: RefOperationStatus::Intent,
            created_at: 1,
            completed_at: None,
        };
        manager
            .storage_ref()
            .unwrap()
            .insert_ref_operation(&record)
            .unwrap();
        let report = manager.reconcile_ref_operations().unwrap();
        assert_eq!(report.abandoned, 1);
        assert_eq!(
            manager
                .git_ref()
                .unwrap()
                .read_ref(&record.ref_name)
                .unwrap()
                .as_deref(),
            Some(head.as_str())
        );
    }

    #[test]
    fn reconcile_is_idempotent() {
        let manager = FileCheckpointManager::new_in_memory().unwrap();
        let report = manager.reconcile_ref_operations().unwrap();
        assert_eq!(report, RefReconcileReport::default());
        assert_eq!(manager.reconcile_ref_operations().unwrap(), report);
        assert!(manager.can_redo("entity-1").unwrap() == false);
    }
}
