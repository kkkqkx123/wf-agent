use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_optimization_opportunities(
    ctx: &ApiContext,
    execution_id: &str,
) -> ApiResult<Vec<OptimizationOpportunity>> {
    let records = get_execution_node_analyses(ctx, execution_id).await?;
    let mut opportunities = Vec::new();

    for record in &records {
        if record.duration.map(|d| d > 5000).unwrap_or(false) {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "Node execution took {}ms - consider optimization",
                    record.duration.unwrap_or(0)
                ),
                impact_level: "medium".to_string(),
                estimated_improvement: Some(format!(
                    "Reduce duration from {}ms",
                    record.duration.unwrap_or(0)
                )),
            });
        }
        if record.retry_count > 2 {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "Node retried {} times - review error handling",
                    record.retry_count
                ),
                impact_level: "high".to_string(),
                estimated_improvement: Some("Improve error handling or node logic".to_string()),
            });
        }
        if record.tool_dependencies.len() > 5 {
            opportunities.push(OptimizationOpportunity {
                node_id: record.node_id.clone(),
                node_name: record.node_name.clone(),
                description: format!(
                    "{} tool dependencies - consider simplification",
                    record.tool_dependencies.len()
                ),
                impact_level: "medium".to_string(),
                estimated_improvement: Some("Reduce tool dependency complexity".to_string()),
            });
        }
    }

    let impact_order = |level: &str| match level {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    };
    opportunities.sort_by_key(|o| impact_order(&o.impact_level));
    Ok(opportunities)
}
