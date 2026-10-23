//! Execution bundle for disaster-recovery review and manual reproduction.
//!
//! A bundle packs what a local reviewer needs after a remote loss: the
//! unified history view, artifact previews and the audit summary. It never
//! promises hot resume on a different host. The same shape serves local
//! backup export and remote pull, so both paths share one format.

use serde::Serialize;

use crate::audit::AuditSummary;
use crate::execution_artifacts::ArtifactEntry;
use crate::execution_history::{history, ExecutionHistorySections, ExecutionHistoryView};
use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

/// Bundle format version. Bump only with an explicit migration path.
pub const EXECUTION_BUNDLE_VERSION: u32 = 1;

/// Portable archive of one execution for local review.
#[derive(Debug, Clone, Serialize)]
pub struct ExecutionBundle {
    pub version: u32,
    pub execution_id: String,
    pub exported_at: i64,
    pub history: ExecutionHistoryView,
    pub artifacts: Vec<ArtifactEntry>,
    pub audit: AuditSummary,
    pub checkpoint_synced: bool,
}

/// Export one execution into a self-contained bundle.
pub async fn export_bundle(
    ctx: &ApiContext,
    execution_id: &str,
    checkpoint_synced: bool,
) -> ApiResult<ExecutionBundle> {
    let history = history(ctx, execution_id, &ExecutionHistorySections::all()).await?;
    let artifacts = crate::execution_artifacts::artifacts_for_execution(ctx, execution_id).await?;
    let audit = crate::audit::audit_summary(ctx, execution_id).await?;
    Ok(ExecutionBundle {
        version: EXECUTION_BUNDLE_VERSION,
        execution_id: execution_id.to_string(),
        exported_at: wf_common::now(),
        history,
        artifacts,
        audit,
        checkpoint_synced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    #[tokio::test]
    async fn unknown_execution_export_fails_not_found() {
        let ctx = ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        );
        let err = export_bundle(&ctx, "missing-bundle", false)
            .await
            .expect_err("unknown execution must not export");
        assert!(err.to_string().contains("missing-bundle"));
    }
}
