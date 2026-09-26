use wf_types::checkpoint::CheckpointTiming;

use super::AgentIterationCoordinator;
use crate::entity::AgentLoopEntity;

impl AgentIterationCoordinator {
    /// Strategy-gated boundary checkpoint; failures only warn so a
    /// checkpoint error never breaks the iteration it snapshots.
    pub(super) async fn boundary_checkpoint(
        &self,
        entity: &AgentLoopEntity,
        trigger: CheckpointTiming,
    ) {
        let Some(ref cp) = self.checkpoint else {
            return;
        };
        if let Err(e) = cp
            .create_checkpoint_gated(entity, trigger.clone(), None)
            .await
        {
            tracing::warn!(
                error = %e,
                entity_id = %entity.id(),
                trigger = ?trigger,
                "failed to create boundary checkpoint"
            );
        }
    }

    /// Message-count backstop: after a batch of appends, checkpoint once
    /// when at least `message_interval` new messages arrived since the last
    /// message-level checkpoint.
    pub(super) async fn maybe_message_checkpoint(&self, entity: &AgentLoopEntity) {
        let Some(interval) = self.message_interval else {
            return;
        };
        if self.checkpoint.is_none() {
            return;
        }
        let len = entity.conversation().read().await.messages().len() as u64;
        let fire = match self.message_checkpoint_watermark.lock() {
            Ok(mut watermark) => {
                if len.saturating_sub(*watermark) >= interval as u64 {
                    *watermark = len;
                    true
                } else {
                    false
                }
            }
            Err(_) => false,
        };
        if fire {
            self.boundary_checkpoint(entity, CheckpointTiming::Interval)
                .await;
        }
    }
}
