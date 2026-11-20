//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.

use crate::file::merge::MergeCommitResult;
use crate::file::FileCheckpointManager;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    pub fn merge_entity_changes(
        &self,
        entity_id: &str,
        feature_name: &str,
    ) -> Result<MergeCommitResult, CheckpointError> {
        crate::branch::ensure_feature_branch_name(feature_name)?;
        let actor = self.actor_id_for(entity_id);
        let (review_ref, review_head) = self.reuse_or_submit_review(entity_id)?;
        let execution = self.merge_review_into_feature_with_behavior(
            &review_ref,
            &review_head,
            feature_name,
            actor.as_str(),
            self.policy.conflict_behavior,
        )?;
        Ok(MergeCommitResult {
            checkpoint_id: execution.outcome.commit_id.clone(),
            merge_result: execution.outcome,
        })
    }
}
