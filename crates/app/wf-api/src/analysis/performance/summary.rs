use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::profile::profile;
use super::helpers::build_summary;
use super::types::*;

pub async fn get_performance_summary(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<PerformanceSummaryView> {
    let profile = profile(ctx, execution_id).await?;
    Ok(build_summary(&profile.timeline, profile.total_duration_ms))
}

