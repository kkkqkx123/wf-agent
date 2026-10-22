//! Approval orchestration on the commit DAG.
//!
//! Write domain: submit, approve, reject and merge review refs into features.
//! Read models live in the top-level approval module.

use wf_types::config::file_checkpoint::ConflictBehavior;

use crate::file::git_merge::GitMergeOutcome;
use crate::file::FileCheckpointManager;
use checkpoint_base::error::CheckpointError;

impl FileCheckpointManager {
    pub fn approval_policy(&self) -> crate::file::ApprovalPolicy {
        self.policy.approval_policy
    }

    /// The configured merge conflict behavior.
    pub fn conflict_behavior(&self) -> ConflictBehavior {
        self.policy.conflict_behavior
    }

    /// Default feature name merged into under `ApprovalPolicy::auto` (a
    /// per-execution feature keeps actors isolated while still landing the
    /// changes into the mainline layer).
    pub fn default_feature_name(entity_id: &str) -> String {
        format!("exec-{}", entity_id)
    }

    /// Agent loop end hook: applies the configured approval policy.
    ///
    /// - `None`: no-op (the actor's edit ref keeps its history and stays
    ///   queryable).
    /// - `Auto`: submit the actor to review and immediately merge into the
    ///   default feature ref.
    /// - `Llm` / `Manual`: submit the actor to review and leave the changes
    ///   pending (`approve_changes` tool / host API resolve them, possibly
    ///   across executions).
    ///
    /// Best-effort by design: the file checkpoint layer must never break an
    /// execution that finished successfully, so failures are reported but
    /// not propagated.
    pub fn on_agent_complete(
        &self,
        entity_id: &str,
    ) -> Result<Option<GitMergeOutcome>, CheckpointError> {
        match self.policy.approval_policy {
            crate::file::ApprovalPolicy::None => Ok(None),
            crate::file::ApprovalPolicy::Auto => {
                let feature = Self::default_feature_name(entity_id);
                match self.merge_entity_changes(entity_id, &feature) {
                    Ok(merged) => Ok(Some(merged.merge_result)),
                    Err(err) => {
                        tracing::warn!("auto approval failed after successful execution: {err}");
                        Ok(None)
                    }
                }
            }
            crate::file::ApprovalPolicy::Llm | crate::file::ApprovalPolicy::Manual => {
                match self.move_agent_to_approval(entity_id) {
                    Ok(_) => Ok(None),
                    Err(err) => {
                        tracing::warn!(
                            "submit for approval failed after successful execution: {err}"
                        );
                        Ok(None)
                    }
                }
            }
        }
    }
}
