use super::types::SearchResultItem;

pub(crate) fn workflow_tags(template: &wf_types::workflow::WorkflowTemplate) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    if let Some(ts) = &template.template_tags {
        tags.extend(ts.iter().cloned());
    }
    if let Some(metadata) = &template.definition.metadata {
        if let Some(ts) = &metadata.tags {
            tags.extend(ts.iter().cloned());
        }
    }
    tags.sort();
    tags.dedup();
    tags
}

/// Sort by relevance (descending) then id (ascending) and truncate.
pub(crate) fn sorted_truncated(
    mut items: Vec<SearchResultItem>,
    limit: usize,
) -> Vec<SearchResultItem> {
    items.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    items.truncate(limit);
    items
}

/// Field values matching the query, in field order.
pub(crate) fn matched_fields(query: &str, fields: &[String]) -> Vec<String> {
    fields
        .iter()
        .filter(|f| f.to_lowercase().contains(query))
        .cloned()
        .collect()
}

/// Simple substring scoring: +2 per matching id, +1 per other matching field.
pub(crate) fn score(query: &str, fields: &[String]) -> u32 {
    if query.is_empty() {
        return 0;
    }
    let mut total = 0;
    for (idx, field) in fields.iter().enumerate() {
        if field.to_lowercase().contains(query) {
            total += if idx == 0 { 2 } else { 1 };
        }
    }
    total
}
