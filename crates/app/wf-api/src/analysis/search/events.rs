use super::scoring::{matched_fields, score, sorted_truncated};
use super::types::SearchResultItem;
use crate::ApiContext;

pub(super) async fn search_events(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> Vec<SearchResultItem> {
    let mut out = Vec::new();
    for event in ctx.event_bus.recent_events() {
        let type_name = event.r#type.as_str().to_lowercase();
        let mut fields = vec![type_name.clone(), event.id.clone()];
        if let Some(workflow_id) = &event.workflow_id {
            fields.push(workflow_id.clone());
        }
        if let Some(execution_id) = &event.execution_id {
            fields.push(execution_id.clone());
        }
        if let Some(agent_loop_id) = &event.agent_loop_id {
            fields.push(agent_loop_id.clone());
        }

        let score = score(query, &fields);
        if score > 0 {
            out.push(SearchResultItem {
                id: event.id.clone(),
                r#type: "event".into(),
                label: format!("Event {}", event.r#type.as_str()),
                description: Some(event.r#type.as_str().to_string()),
                score,
                matches: matched_fields(query, &fields),
                execution_id: event.execution_id.clone(),
                agent_loop_id: event.agent_loop_id.clone(),
            });
        }
    }
    sorted_truncated(out, limit)
}
