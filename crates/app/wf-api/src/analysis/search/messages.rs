use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;
use crate::ApiContext;
use crate::ApiResult;

pub(super) async fn search_messages(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let records = crate::entity::message::search(ctx, query, Some(limit)).await?;
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let text = crate::entity::message::message_text(&record.message);
        let fields = vec![record.id.clone(), record.execution_id.clone(), text.clone()];
        let item_score = score(query, &fields).max(1);
        let snippet: String = text.chars().take(80).collect();
        let label = format!("{}: {snippet}", record.id);
        out.push(SearchResultItem {
            id: record.id.clone(),
            r#type: "message".into(),
            label,
            description: Some(snippet),
            score: item_score,
            matches: matched_fields(query, &fields),
            execution_id: Some(record.execution_id.clone()),
            agent_loop_id: record.agent_loop_id.clone(),
        });
    }
    Ok(sorted_truncated(out, limit))
}
