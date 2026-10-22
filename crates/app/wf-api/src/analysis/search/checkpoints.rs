use wf_storage::adapter::base::BaseStorageAdapter;
use crate::ApiContext;
use crate::ApiResult;
use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;

pub(super) async fn search_checkpoints(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.checkpoint.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.entity_id.clone(),
            entity.entity_type.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "checkpoint".into(),
                label: format!("{} (entity {})", entity.id, entity.entity_id),
                description: Some(format!("{} / {}", entity.entity_type, entity.entity_id)),
                score,
                matches: matched_fields(query, &fields),
                execution_id: None,
                agent_loop_id: None,
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}

