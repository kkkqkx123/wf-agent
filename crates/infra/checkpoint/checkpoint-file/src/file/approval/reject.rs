//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.


use crate::event::{ApprovalDecision, CheckpointEventBus};
use crate::file::git_write::map_git_error;
use crate::file::FileCheckpointManager;
use crate::git_store::REF_REVIEW_PREFIX;
use crate::storage::ReviewStatus;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    pub fn reject_changes(
        &self,
        entity_id: &str,
        reason: Option<&str>,
    ) -> Result<String, CheckpointError> {
        let actor = self.actor_id_for(entity_id);
        let swept = self.reconcile_reviews(entity_id)?;
        let git = self.git_ref()?;
        let storage = self.storage_ref()?;
        let mut targets: Vec<(String, String)> = Vec::new();
        for (review_ref, _) in git.list_refs(REF_REVIEW_PREFIX).map_err(map_git_error)? {
            if storage.get_review_state(&review_ref)? != Some(ReviewStatus::Pending) {
                continue;
            }
            let Some(head) = git.read_ref(&review_ref).map_err(map_git_error)? else {
                tracing::warn!(review = %review_ref, "reject scan found headless review; skipping");
                continue;
            };
            let Ok(commit) = git.read_commit(&head) else {
                tracing::warn!(review = %review_ref, "reject scan found unreadable review; skipping");
                continue;
            };
            if commit.trailer(crate::git_store::TRAILER_ACTOR).as_deref() != Some(actor.as_str()) {
                continue;
            }
            targets.push((review_ref, head));
        }
        let mut deleted: Vec<String> = Vec::new();
        for (review_ref, head) in targets {
            git.delete_ref(&review_ref).map_err(map_git_error)?;
            storage.set_review_state(&review_ref, ReviewStatus::Rejected)?;
            deleted.push(head);
        }
        let last = deleted
            .into_iter()
            .next_back()
            .ok_or_else(|| CheckpointError::Validation {
                reason: format!("actor '{}' has no pending submission", actor.as_str()),
            })?;
        if let Some(ref bus) = self.event_bus {
            bus.publish(CheckpointEventBus::approval_resolved(
                actor.as_str().to_string(),
                ApprovalDecision::Rejected,
                last.clone(),
            ));
        }
        match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(reason) => tracing::info!(
                entity = %entity_id,
                baseline = %last,
                reason = %reason,
                reconciled_orphans = swept,
                "rejected pending approval",
            ),
            None => tracing::info!(
                entity = %entity_id,
                baseline = %last,
                reconciled_orphans = swept,
                "rejected pending approval without a reason",
            ),
        }
        Ok(last)
    }
}
