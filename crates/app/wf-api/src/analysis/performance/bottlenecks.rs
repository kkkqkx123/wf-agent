use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::helpers::build_bottleneck_views;
use super::profile::profile;
use super::types::*;

pub async fn identify_bottlenecks(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<PerformanceBottleneckView>> {
    let profile = profile(ctx, execution_id).await?;
    Ok(build_bottleneck_views(
        &profile.timeline,
        profile.total_duration_ms,
    ))
}
