use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;
use crate::ApiContext;
use crate::ApiResult;
use wf_storage::adapter::base::BaseStorageAdapter;

pub(super) async fn search_tasks(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.task.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.task_type.clone(),
            entity.status.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "task".into(),
                label: format!("{} ({})", entity.task_type, entity.status),
                description: Some(format!("{} / {}", entity.task_type, entity.status)),
                score,
                matches: matched_fields(query, &fields),
                execution_id: entity.execution_id.clone(),
                agent_loop_id: None,
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}
