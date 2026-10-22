use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;
use crate::ApiContext;
use crate::ApiResult;
use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;

pub(super) async fn search_executions(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.workflow_execution.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let workflow_name = ctx
            .registries
            .workflows
            .get(&entity.workflow_id)
            .map(|template| template.name.clone())
            .unwrap_or_default();
        let mut fields = vec![
            entity.id.clone(),
            entity.workflow_id.clone(),
            entity.status.as_str().to_string(),
        ];
        if !workflow_name.is_empty() {
            fields.push(workflow_name);
        }
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "execution".into(),
                label: format!("{} (workflow {})", entity.id, entity.workflow_id),
                description: Some(format!("status: {}", entity.status.as_str())),
                score,
                matches: matched_fields(query, &fields),
                execution_id: Some(entity.id.clone()),
                agent_loop_id: None,
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}


