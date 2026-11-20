//! Periodic execution retention sweep.

use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};
use wf_types::config::RetentionConfig;

/// Start the automatic sweep. The task stays armed so a policy change made
/// through the settings API takes effect without a runtime restart; every
/// tick re-reads the effective policy and skips disabled or report-only runs.
pub fn spawn_retention_timer(
    ctx: Arc<wf_api::ApiContext>,
    config: RetentionConfig,
) -> Option<tokio::task::JoinHandle<()>> {
    let interval = Duration::from_secs(config.scan_interval_secs.max(1));
    info!(
        interval_secs = config.scan_interval_secs,
        "Retention sweep timer started"
    );
    Some(wf_common::spawn_ticker(
        interval,
        tokio_util::sync::CancellationToken::new(),
        move || {
            let ctx = Arc::clone(&ctx);
            async move {
                let policy = match wf_api::retention::settings::current(&ctx).await {
                    Ok(policy) => policy,
                    Err(err) => {
                        warn!(error = %err, "Retention policy unreadable; sweep skipped");
                        return;
                    }
                };
                if !policy.enabled || policy.dry_run {
                    return;
                }
                match wf_api::retention::scan(&ctx, false).await {
                    Ok(report) => info!(
                        scanned = report.scanned,
                        archived = report.executions_archived,
                        deleted = report.executions_deleted,
                        checkpoints = report.checkpoints_removed,
                        "Retention sweep completed"
                    ),
                    Err(err) => warn!(error = %err, "Retention sweep failed"),
                }
            }
        },
    ))
}
