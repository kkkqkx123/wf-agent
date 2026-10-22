//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.


use crate::approval::PendingApproval;
use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
use crate::git_store::REF_REVIEW_PREFIX;
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    pub fn list_pending_approvals(&self) -> Result<Vec<PendingApproval>, CheckpointError> {
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut views = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            match storage.get_review_state(&review_ref)? {
                Some(ReviewStatus::Pending) => {}
                None => {
                    tracing::warn!(review = %review_ref, "pending scan found ref without state row; skipping");
                    continue;
                }
                _ => continue,
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                tracing::warn!(review = %review_ref, "pending review ref has no head; skipping");
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                tracing::warn!(review = %review_ref, head = %head, "pending review head unreadable; skipping");
                continue;
            };
            let actor = commit
                .trailer(crate::git_store::TRAILER_ACTOR)
                .unwrap_or_default();
            // The actor edit line is a disconnected root, so the baseline for
            // "submitted changes" is the main ref tree (the workspace state
            // before the actor's edits). This yields correct Added/Modified/
            // Deleted kinds instead of a per-commit delta that would only
            // show the last commit's files.
            let baseline = match git
                .read_ref(crate::git_store::REF_MAIN)
                .map_err(map_git_error)?
            {
                Some(main_head) => git
                    .read_commit(&main_head)
                    .ok()
                    .map(|c| git.tree_to_bytes(&c.tree))
                    .transpose()
                    .map_err(map_git_error)?,
                None => None,
            };
            let changes = crate::provenance::tree_delta(git, baseline.as_ref(), &commit)?;
            views.push(PendingApproval {
                actor,
                snapshot_id: head,
                submitted_at: commit.committer_ts,
                changes,
            });
        }
        views.sort_by_key(|a| a.submitted_at);
        Ok(views)
    }

    /// Sweep half-rejected leftovers for an actor: pending state rows whose
    /// review ref no longer exists (a previous reject deleted the ref but
    /// failed before deleting the row). The rows are deleted and counted so
    /// a repeated reject converges instead of leaving invisible pending
    /// state behind. Only rows under this actor's review-ref prefix are
    /// touched, matching the submission naming scheme.
    pub fn reconcile_reviews(&self, entity_id: &str) -> Result<usize, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let prefix = format!(
            "{REF_REVIEW_PREFIX}{}-",
            crate::git_store::sanitize_ref_component(actor.as_str())
        );
        let mut swept = 0usize;
        for (review_ref, status) in storage.list_review_states()? {
            if status != ReviewStatus::Pending || !review_ref.starts_with(&prefix) {
                continue;
            }
            let headless = git.read_ref(&review_ref).map_err(map_git_error)?.is_none();
            if headless {
                tracing::warn!(review = %review_ref, "reconciling orphan pending state row; ref already gone");
                storage.delete_review_state(&review_ref)?;
                swept += 1;
            }
        }
        Ok(swept)
    }
}
