use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;
use crate::ApiContext;
use crate::ApiResult;
use wf_storage::adapter::base::BaseStorageAdapter;

pub(super) async fn search_agent_loops(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let entities = ctx.storage.agent_loop.list(None).await?;
    let mut out = Vec::new();
    for entity in entities {
        let fields = [
            entity.id.clone(),
            entity.definition_id.clone(),
            entity.status.clone(),
        ];
        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: entity.id.clone(),
                r#type: "agent_loop".into(),
                label: format!("{} (definition {})", entity.id, entity.definition_id),
                description: Some(format!("{} / {}", entity.definition_id, entity.status)),
                score,
                matches: matched_fields(query, &fields),
                execution_id: Some(entity.id.clone()),
                agent_loop_id: Some(entity.id.clone()),
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}
