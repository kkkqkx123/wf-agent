use super::scoring::{matched_fields, score, sorted_truncated, workflow_tags};
use super::types::SearchResultItem;
use crate::ApiContext;
use crate::ApiResult;
use wf_core::registry::Registry;

pub(super) async fn search_workflows(
    ctx: &ApiContext,
    query: &str,
    limit: usize,
) -> ApiResult<Vec<SearchResultItem>> {
    let mut out = Vec::new();
    for id in ctx.registries.workflows.list() {
        let Some(template) = ctx.registries.workflows.get(&id) else {
            continue;
        };

        let mut fields = vec![id.clone(), template.name.clone()];
        if !template.description.is_empty() {
            fields.push(template.description.clone());
        }
        if let Some(desc) = &template.definition.description {
            fields.push(desc.clone());
        }
        for tag in workflow_tags(&template) {
            fields.push(tag);
        }

        let score = score(query, &fields);
        if score > 0 {
            let description = if !template.description.is_empty() {
                Some(template.description.clone())
            } else {
                template.definition.description.clone()
            };
            out.push(SearchResultItem {
                id: id.clone(),
                r#type: "workflow".into(),
                label: template.name.clone(),
                description,
                score,
                matches: matched_fields(query, &fields),
                execution_id: None,
                agent_loop_id: None,
            });
        }
    }
    Ok(sorted_truncated(out, limit))
}
