use wf_storage::adapter::base::BaseStorageAdapter;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;

use super::*;

pub async fn query(ctx: &ApiContext, filter: &TemplateFilter) -> ApiResult<Vec<TemplateSummary>> {
    let mut summaries = Vec::new();
    let include_workflow = filter.kind.is_none_or(|k| k == TemplateKind::Workflow);
    let include_agent = filter.kind.is_none_or(|k| k == TemplateKind::Agent);
    let usage = super::usage::usage_map(ctx).await?;

    if include_workflow {
        summaries.extend(list_workflow_templates(ctx)?.into_iter().map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            super::summary::summary_from_workflow(t, count)
        }));
    }
    if include_agent {
        summaries.extend(list_agent_templates(ctx)?.into_iter().map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            super::summary::summary_from_agent(t, count)
        }));
    }

    let basic = crate::template::BasicTemplateFilter {
        name: filter.name.clone(),
        category: filter.category.clone(),
        tags: filter.tags.clone(),
        author: filter.author.clone(),
    };

    let include_system = filter.include_system.unwrap_or(false);
    Ok(summaries
        .into_iter()
        .filter(|s| {
            if !include_system && s.is_system {
                return false;
            }
            basic.matches(
                &s.name,
                s.category.as_deref(),
                s.tags.as_deref(),
                s.author.as_deref(),
            )
        })
        .collect())
}

pub async fn query_by_category(
    ctx: &ApiContext,
    category: &str,
) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            category: Some(category.to_string()),
            ..TemplateFilter::default()
        },
    )
    .await
}

pub async fn query_by_tags(ctx: &ApiContext, tags: &[String]) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            tags: Some(tags.to_vec()),
            ..TemplateFilter::default()
        },
    )
    .await
}

pub async fn query_by_author(ctx: &ApiContext, author: &str) -> ApiResult<Vec<TemplateSummary>> {
    query(
        ctx,
        &TemplateFilter {
            author: Some(author.to_string()),
            ..TemplateFilter::default()
        },
    )
    .await
}

/// Unified query across all four template kinds. A missing kind means all.
/// Workflow and agent entries come from the in-memory library, node and
/// trigger entries from storage. Keeps the single-request library surface.
pub async fn query_unified(
    ctx: &ApiContext,
    filter: &TemplateFilter,
) -> ApiResult<Vec<TemplateSummary>> {
    let include_workflow = filter.kind.is_none_or(|k| k == TemplateKind::Workflow);
    let include_agent = filter.kind.is_none_or(|k| k == TemplateKind::Agent);
    let include_node = filter.kind.is_none_or(|k| k == TemplateKind::Node);
    let include_trigger = filter.kind.is_none_or(|k| k == TemplateKind::Trigger);

    let mut summaries = Vec::new();
    if include_workflow || include_agent {
        let library_filter = TemplateFilter {
            kind: if include_workflow && include_agent {
                None
            } else if include_workflow {
                Some(TemplateKind::Workflow)
            } else {
                Some(TemplateKind::Agent)
            },
            name: filter.name.clone(),
            category: filter.category.clone(),
            tags: filter.tags.clone(),
            author: filter.author.clone(),
            include_system: filter.include_system,
        };
        summaries.extend(query(ctx, &library_filter).await?);
    }
    let basic = crate::template::BasicTemplateFilter {
        name: filter.name.clone(),
        category: filter.category.clone(),
        tags: filter.tags.clone(),
        author: filter.author.clone(),
    };
    if include_node {
        let nodes = ctx.storage.node_template.list(None).await?;
        let usage = super::usage::usage_map(ctx).await?;
        summaries.extend(nodes.into_iter().filter_map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            let summary = super::summary::summary_from_node(t, count);
            basic
                .matches(
                    &summary.name,
                    summary.category.as_deref(),
                    summary.tags.as_deref(),
                    summary.author.as_deref(),
                )
                .then_some(summary)
        }));
    }
    if include_trigger {
        let triggers = ctx.storage.trigger_template.list(None).await?;
        let usage = super::usage::usage_map(ctx).await?;
        summaries.extend(triggers.into_iter().filter_map(|t| {
            let count = usage.get(&t.id.to_string()).copied().unwrap_or(0);
            let summary = super::summary::summary_from_trigger(t, count);
            basic
                .matches(
                    &summary.name,
                    summary.category.as_deref(),
                    summary.tags.as_deref(),
                    summary.author.as_deref(),
                )
                .then_some(summary)
        }));
    }
    Ok(summaries)
}

/// Featured templates: public + enabled, most used first.
pub async fn featured(ctx: &ApiContext, limit: Option<usize>) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query(ctx, &TemplateFilter::default())
        .await?
        .into_iter()
        .filter(|t| t.is_public && t.enabled && !t.is_system)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}

/// Popular templates within a category, most used first.
pub async fn popular_in_category(
    ctx: &ApiContext,
    category: &str,
    limit: Option<usize>,
) -> ApiResult<Vec<TemplateSummary>> {
    let mut all = query_by_category(ctx, category)
        .await?
        .into_iter()
        .filter(|t| t.enabled)
        .collect::<Vec<_>>();
    all.sort_by_key(|t| std::cmp::Reverse(t.usage_count));
    all.truncate(limit.unwrap_or(DEFAULT_FEATURED_LIMIT));
    Ok(all)
}
