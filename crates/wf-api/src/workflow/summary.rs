//! Workflow summary projection and listing.

use wf_types::WorkflowDefinition;

/// Digest of a workflow summary fields.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub node_count: usize,
    pub edge_count: usize,
    pub updated_at: i64,
}

/// Project a workflow definition onto WorkflowSummary.
pub fn to_summary(wf: WorkflowDefinition) -> WorkflowSummary {
    WorkflowSummary {
        id: wf.id.clone(),
        name: wf.name.clone(),
        description: wf.description.clone(),
        version: wf.version.clone(),
        node_count: wf.nodes.len(),
        edge_count: wf.edges.len(),
        updated_at: wf.updated_at,
    }
}

/// Project list_workflows results onto WorkflowSummary.
pub async fn workflow_summaries(
    ctx: &crate::ApiContext,
    options: Option<wf_storage::adapter::workflow::WorkflowListOptions>,
) -> crate::ApiResult<Vec<WorkflowSummary>> {
    Ok(super::definition::list_workflows(ctx, options)
        .await?
        .into_iter()
        .map(to_summary)
        .collect())
}
