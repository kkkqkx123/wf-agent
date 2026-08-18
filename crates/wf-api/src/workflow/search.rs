//! Workflow search and filtering by keyword, tags, category, author.

use crate::ApiContext;
use wf_types::WorkflowDefinition;

use super::definition::list_workflows;
use super::summary::{to_summary, WorkflowSummary};

/// Search options over workflow definitions.
#[derive(Debug, Clone, Default)]
pub struct WorkflowSearchOptions {
    pub keyword: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<String>,
    pub author: Option<String>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

/// Search workflows by name/description/tags/category/author, with pagination.
pub async fn search_workflows(
    ctx: &ApiContext,
    options: &WorkflowSearchOptions,
) -> crate::ApiResult<Vec<WorkflowSummary>> {
    let keyword = options
        .keyword
        .as_deref()
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty());
    let mut matches: Vec<WorkflowDefinition> = list_workflows(ctx, None).await?;
    matches.retain(|wf| workflow_matches(wf, options, keyword.as_deref()));

    let offset = options.offset.unwrap_or(0) as usize;
    let limit = options.limit.map(|l| l as usize);
    let page: Vec<WorkflowDefinition> = matches
        .into_iter()
        .skip(offset)
        .take(limit.unwrap_or(usize::MAX))
        .collect();
    Ok(page.into_iter().map(to_summary).collect())
}

/// The workflow with the given name, or `None`.
pub async fn get_workflow_by_name(
    ctx: &ApiContext,
    name: &str,
) -> crate::ApiResult<Option<WorkflowDefinition>> {
    Ok(list_workflows(ctx, None)
        .await?
        .into_iter()
        .find(|wf| wf.name == name))
}

/// Workflows carrying every given tag.
pub async fn get_workflows_by_tags(
    ctx: &ApiContext,
    tags: &[String],
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    Ok(list_workflows(ctx, None)
        .await?
        .into_iter()
        .filter(|wf| {
            tags.is_empty()
                || wf
                    .metadata
                    .as_ref()
                    .and_then(|m| m.tags.as_ref())
                    .map(|existing| tags.iter().all(|tag| existing.contains(tag)))
                    .unwrap_or(false)
        })
        .collect())
}

/// Workflows in a category.
pub async fn get_workflows_by_category(
    ctx: &ApiContext,
    category: &str,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    Ok(list_workflows(ctx, None)
        .await?
        .into_iter()
        .filter(|wf| wf.metadata.as_ref().and_then(|m| m.category.as_deref()) == Some(category))
        .collect())
}

/// Workflows by author.
pub async fn get_workflows_by_author(
    ctx: &ApiContext,
    author: &str,
) -> crate::ApiResult<Vec<WorkflowDefinition>> {
    Ok(list_workflows(ctx, None)
        .await?
        .into_iter()
        .filter(|wf| wf.metadata.as_ref().and_then(|m| m.author.as_deref()) == Some(author))
        .collect())
}

fn workflow_matches(
    workflow: &WorkflowDefinition,
    options: &WorkflowSearchOptions,
    keyword: Option<&str>,
) -> bool {
    if let Some(keyword) = keyword {
        let mut haystack = vec![workflow.id.to_lowercase(), workflow.name.to_lowercase()];
        if let Some(description) = &workflow.description {
            haystack.push(description.to_lowercase());
        }
        if let Some(metadata) = &workflow.metadata {
            if let Some(author) = &metadata.author {
                haystack.push(author.to_lowercase());
            }
            if let Some(category) = &metadata.category {
                haystack.push(category.to_lowercase());
            }
            if let Some(tags) = &metadata.tags {
                haystack.extend(tags.iter().map(|t| t.to_lowercase()));
            }
        }
        if !haystack.iter().any(|field| field.contains(keyword)) {
            return false;
        }
    }
    if let Some(tags) = &options.tags {
        let present = workflow
            .metadata
            .as_ref()
            .and_then(|m| m.tags.as_ref())
            .map(|existing| tags.iter().all(|tag| existing.contains(tag)))
            .unwrap_or(false);
        if !present {
            return false;
        }
    }
    if let Some(category) = &options.category {
        if workflow
            .metadata
            .as_ref()
            .and_then(|m| m.category.as_deref())
            != Some(category.as_str())
        {
            return false;
        }
    }
    if let Some(author) = &options.author {
        if workflow.metadata.as_ref().and_then(|m| m.author.as_deref()) != Some(author.as_str()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::definition::save_workflow;
    use wf_common;
    use wf_types::workflow::WorkflowMetadata;
    use wf_resource::registry::ResourceRegistries;
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use std::sync::Arc;

    fn make_ctx() -> ApiContext {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        let bundles = Arc::new(ResourcePluginRegistry::new());
        ApiContext::new(storage, registries, bundles)
    }

    fn make_workflow_with_meta(
        id: &str,
        name: &str,
        description: Option<&str>,
        metadata: Option<WorkflowMetadata>,
    ) -> WorkflowDefinition {
        WorkflowDefinition {
            id: id.into(),
            name: name.into(),
            description: description.map(String::from),
            r#type: None,
            version: Some("1.0.0".into()),
            nodes: vec![
                wf_types::node::BaseStaticNode {
                    id: "start".into(),
                    node_type: wf_types::node::StaticNodeType::Start,
                    name: Some("start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                wf_types::node::BaseStaticNode {
                    id: "end".into(),
                    node_type: wf_types::node::StaticNodeType::End,
                    name: Some("end".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![wf_types::workflow::Edge {
                id: "e1".into(),
                source_node_id: "start".into(),
                target_node_id: "end".into(),
                r#type: wf_types::workflow::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            }],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata,
            available_tools: None,
            hooks: None,
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        }
    }

    #[tokio::test]
    async fn test_search_workflows_by_keyword_tags_category_author() {
        let ctx = make_ctx();
        save_workflow(
            &ctx,
            &make_workflow_with_meta(
                "wf-a",
                "Order Processing",
                Some("handles orders"),
                Some(WorkflowMetadata {
                    author: Some("alice".into()),
                    tags: Some(vec!["ecommerce".into(), "core".into()]),
                    category: Some("sales".into()),
                }),
            ),
        )
        .await
        .unwrap();
        save_workflow(
            &ctx,
            &make_workflow_with_meta(
                "wf-b",
                "Inventory Sync",
                None,
                Some(WorkflowMetadata {
                    author: Some("bob".into()),
                    tags: Some(vec!["ecommerce".into()]),
                    category: Some("ops".into()),
                }),
            ),
        )
        .await
        .unwrap();
        save_workflow(&ctx, &make_workflow_with_meta("wf-c", "Workflow c", None, None)).await.unwrap();

        let found = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                keyword: Some("inventory".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "wf-b");

        let by_tag = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                tags: Some(vec!["ecommerce".into(), "core".into()]),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_tag.len(), 1);
        assert_eq!(by_tag[0].id, "wf-a");

        let by_category = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                category: Some("ops".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_category.len(), 1);

        let by_author = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                author: Some("bob".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_author.len(), 1);

        let page1 = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let page2 = search_workflows(
            &ctx,
            &WorkflowSearchOptions {
                offset: Some(1),
                limit: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(page1.len(), 1);
        assert_eq!(page2.len(), 1);
        assert_ne!(page1[0].id, page2[0].id);
    }

    #[tokio::test]
    async fn test_get_workflow_by_name_and_by_tags_category_author() {
        let ctx = make_ctx();
        save_workflow(
            &ctx,
            &make_workflow_with_meta(
                "wf-x",
                "Unique Name",
                None,
                Some(WorkflowMetadata {
                    author: Some("carol".into()),
                    tags: Some(vec!["alpha".into(), "beta".into()]),
                    category: Some("finance".into()),
                }),
            ),
        )
        .await
        .unwrap();
        save_workflow(
            &ctx,
            &make_workflow_with_meta(
                "wf-y",
                "Other",
                None,
                Some(WorkflowMetadata {
                    author: Some("carol".into()),
                    tags: Some(vec!["beta".into()]),
                    category: Some("hr".into()),
                }),
            ),
        )
        .await
        .unwrap();

        let by_name = get_workflow_by_name(&ctx, "Unique Name")
            .await
            .unwrap()
            .expect("found by name");
        assert_eq!(by_name.id, "wf-x");
        assert!(get_workflow_by_name(&ctx, "missing")
            .await
            .unwrap()
            .is_none());

        let by_tags = get_workflows_by_tags(&ctx, &["beta".to_string()])
            .await
            .unwrap();
        assert_eq!(by_tags.len(), 2);
        let all_tags = get_workflows_by_tags(&ctx, &["alpha".to_string(), "beta".to_string()])
            .await
            .unwrap();
        assert_eq!(all_tags.len(), 1);
        assert!(get_workflows_by_tags(&ctx, &["missing".to_string()])
            .await
            .unwrap()
            .is_empty());

        let by_category = get_workflows_by_category(&ctx, "finance").await.unwrap();
        assert_eq!(by_category.len(), 1);
        assert_eq!(by_category[0].id, "wf-x");

        let by_author = get_workflows_by_author(&ctx, "carol").await.unwrap();
        assert_eq!(by_author.len(), 2);
    }
}
