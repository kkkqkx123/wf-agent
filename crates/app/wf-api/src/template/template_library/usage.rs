use std::collections::HashMap;


use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub(super) async fn usage_map(ctx: &ApiContext) -> ApiResult<HashMap<String, u64>> {
    use wf_storage::adapter::base::BaseStorageAdapter;
    let records = ctx.storage.template_usage.list(None).await?;
    Ok(records
        .into_iter()
        .map(|record| (record.template_id, record.count))
        .collect())
}

/// Resolve the stored kind of a template id for first-use record creation.
pub(super) async fn resolve_usage_kind(ctx: &ApiContext, id: &str) -> String {
    if ctx.registries.workflows.has(id) {
        return TemplateKind::Workflow.as_str().to_string();
    }
    if ctx.registries.agent_templates.has(id) {
        return TemplateKind::Agent.as_str().to_string();
    }
    use wf_storage::adapter::base::BaseStorageAdapter;
    if ctx
        .storage
        .node_template
        .load(id)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return TemplateKind::Node.as_str().to_string();
    }
    if ctx
        .storage
        .trigger_template
        .load(id)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        return TemplateKind::Trigger.as_str().to_string();
    }
    TemplateKind::Workflow.as_str().to_string()
}

/// Increment the usage counter of a template (any kind) and return the new
/// count. Called explicitly when a template is instantiated, cloned, or
/// exported; repeated calls each count once.
pub async fn record_usage(ctx: &ApiContext, id: &str) -> ApiResult<u64> {
    use wf_storage::adapter::template_usage::TemplateUsageStorageAdapter;
    let kind = resolve_usage_kind(ctx, id).await;
    ctx.storage
        .template_usage
        .increment(id, &kind)
        .await
        .map_err(crate::ApiError::from)
}

/// Current usage count of a template; zero when never recorded or when the
/// store read fails, so library queries stay robust.
pub async fn usage_count(ctx: &ApiContext, id: &str) -> u64 {
    use wf_storage::adapter::template_usage::TemplateUsageStorageAdapter;
    ctx.storage.template_usage.get_count(id).await.unwrap_or(0)
}

