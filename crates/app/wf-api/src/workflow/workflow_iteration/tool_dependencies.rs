

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn get_tool_dependency_chain(
    ctx: &ApiContext,
    execution_id: &str,
    node_id: &str,
) -> ApiResult<Vec<ToolDependencyView>> {
    Ok(get_node_analysis(ctx, execution_id, node_id)
        .await?
        .map(|record| record.tool_dependencies)
        .unwrap_or_default())
}

pub(super) fn is_tool_node(node_type: &str) -> bool {
    let upper = node_type.to_uppercase();
    upper == "TOOL" || upper == "HTTP" || upper == "MCP" || upper == "SCRIPT"
}

