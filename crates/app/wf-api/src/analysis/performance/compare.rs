use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::profile::profile;
use super::types::*;

pub async fn compare(
    ctx: &ApiContext,
    baseline_id: &str,
    compared_id: &str,
) -> ApiResult<ExecutionComparison> {
    let baseline = profile(ctx, baseline_id).await?;
    let compared = profile(ctx, compared_id).await?;
    let duration_change_ms = compared.total_duration_ms - baseline.total_duration_ms;
    let improvement_rate = if baseline.total_duration_ms > 0 {
        (baseline.total_duration_ms - compared.total_duration_ms) as f64
            / baseline.total_duration_ms as f64
    } else {
        0.0
    };
    Ok(ExecutionComparison {
        baseline_id: baseline_id.to_string(),
        compared_id: compared_id.to_string(),
        baseline_duration_ms: baseline.total_duration_ms,
        compared_duration_ms: compared.total_duration_ms,
        duration_change_ms,
        improvement_rate: improvement_rate.clamp(-1.0, 1.0),
        improved: improvement_rate > 0.0,
    })
}

