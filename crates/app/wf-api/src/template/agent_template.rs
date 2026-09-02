use wf_types::agent::AgentTemplate;

use crate::infra::context::ApiContext;
use crate::infra::error::ApiResult;
use crate::template::template_library::{TemplateFilter, TemplateSummary};

/// Agent template filter.
#[derive(Debug, Clone, Default)]
pub struct AgentTemplateFilter {
    pub name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
    pub profile_type: Option<String>,
}

/// Query agent templates with an optional filter.
pub fn query(
    ctx: &ApiContext,
    filter: Option<&AgentTemplateFilter>,
) -> ApiResult<Vec<AgentTemplate>> {
    let all = list_agent_templates(ctx)?;
    Ok(all
        .into_iter()
        .filter(|t| {
            let Some(filter) = filter else {
                return true;
            };
            let basic = crate::template::BasicTemplateFilter {
                name: filter.name.clone(),
                category: filter.category.clone(),
                tags: filter.tags.clone(),
                author: filter.author.clone(),
            };
            let category = t.template_category.clone().or_else(|| {
                t.definition
                    .metadata
                    .as_ref()
                    .and_then(|m| m.category.clone())
            });
            let tags = t
                .template_tags
                .clone()
                .or_else(|| t.definition.metadata.as_ref().and_then(|m| m.tags.clone()));
            let author = t
                .definition
                .metadata
                .as_ref()
                .and_then(|m| m.author.as_ref());
            if !basic.matches(
                &t.name,
                category.as_deref(),
                tags.as_deref(),
                author.map(String::as_str),
            ) {
                return false;
            }
            if let Some(profile_type) = &filter.profile_type {
                let matches = t
                    .definition
                    .config
                    .as_ref()
                    .and_then(|c| c.profile_id.clone())
                    .map(|p| p == *profile_type)
                    .unwrap_or(false);
                if !matches {
                    return false;
                }
            }
            true
        })
        .collect())
}

pub fn query_by_category(ctx: &ApiContext, category: &str) -> ApiResult<Vec<AgentTemplate>> {
    query(
        ctx,
        Some(&AgentTemplateFilter {
            category: Some(category.to_string()),
            ..AgentTemplateFilter::default()
        }),
    )
}

pub fn query_by_tags(ctx: &ApiContext, tags: &[String]) -> ApiResult<Vec<AgentTemplate>> {
    query(
        ctx,
        Some(&AgentTemplateFilter {
            tags: Some(tags.to_vec()),
            ..AgentTemplateFilter::default()
        }),
    )
}

pub fn query_by_author(ctx: &ApiContext, author: &str) -> ApiResult<Vec<AgentTemplate>> {
    query(
        ctx,
        Some(&AgentTemplateFilter {
            author: Some(author.to_string()),
            ..AgentTemplateFilter::default()
        }),
    )
}

pub fn query_by_profile_type(
    ctx: &ApiContext,
    profile_type: &str,
) -> ApiResult<Vec<AgentTemplate>> {
    query(
        ctx,
        Some(&AgentTemplateFilter {
            profile_type: Some(profile_type.to_string()),
            ..AgentTemplateFilter::default()
        }),
    )
}

/// Featured templates: public + enabled, most used first.
pub fn featured(ctx: &ApiContext, limit: Option<usize>) -> ApiResult<Vec<TemplateSummary>> {
    Ok(crate::template::template_library::featured(ctx, limit)?
        .into_iter()
        .filter(|t| t.kind == "agent")
        .collect())
}

/// Popular templates within a category, most used first.
pub fn popular_in_category(
    ctx: &ApiContext,
    category: &str,
    limit: Option<usize>,
) -> ApiResult<Vec<TemplateSummary>> {
    Ok(
        crate::template::template_library::popular_in_category(ctx, category, limit)?
            .into_iter()
            .filter(|t| t.kind == "agent")
            .collect(),
    )
}

/// Uniform summaries across the matching agent templates.
pub fn summaries(
    ctx: &ApiContext,
    filter: Option<&AgentTemplateFilter>,
) -> ApiResult<Vec<TemplateSummary>> {
    let ids: Vec<String> = query(ctx, filter)?
        .into_iter()
        .map(|t| t.id.to_string())
        .collect();
    Ok(crate::template::template_library::query(
        ctx,
        &TemplateFilter {
            kind: Some(crate::template::template_library::TemplateKind::Agent),
            ..TemplateFilter::default()
        },
    )?
    .into_iter()
    .filter(|s| ids.contains(&s.id))
    .collect())
}

fn list_agent_templates(ctx: &ApiContext) -> ApiResult<Vec<AgentTemplate>> {
    crate::template::template_library::list_agent_templates(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::{register_item_skip, ResourceRegistries};
    use wf_resource::resource_plugin::ResourcePluginRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::agent::{AgentConfig, AgentDefinition, AgentMetadata};

    fn agent_template(id: &str, category: &str, profile_id: &str) -> AgentTemplate {
        let now = wf_common::now();
        AgentTemplate {
            id: id.into(),
            name: format!("Agent {id}"),
            description: format!("Description {id}"),
            definition: AgentDefinition {
                id: id.into(),
                name: format!("Agent {id}"),
                description: None,
                version: None,
                config: Some(AgentConfig {
                    profile_id: Some(profile_id.to_string()),
                    system_prompt: None,
                    system_prompt_template_id: None,
                    system_prompt_template_variables: None,
                    max_iterations: None,
                    max_execution_time: None,
                    max_retries: None,
                    execution_timeout: None,
                    max_pause_duration: None,
                    token_limit: None,
                    token_warning_threshold: None,
                    enable_token_tracking: None,
                    initial_messages: None,
                    available_tools: None,
                    stream: None,
                    tool_call_format: None,
                    hooks: None,
                    dynamic_context: None,
                    checkpoint: None,
                    violation_policy: None,
                }),
                metadata: Some(AgentMetadata {
                    author: Some("author-x".into()),
                    tags: Some(vec!["tag-b".into()]),
                    category: Some(category.into()),
                }),
                created_at: now,
                updated_at: now,
            },
            template_category: Some(category.into()),
            template_tags: Some(vec!["tag-b".into()]),
            is_public: Some(true),
            enabled: Some(true),
        }
    }

    fn make_ctx() -> Arc<ApiContext> {
        let registries = Arc::new(ResourceRegistries::new());
        register_item_skip(
            &registries.agent_templates,
            "agent-a".into(),
            agent_template("agent-a", "analytics", "profile-gpt"),
        );
        register_item_skip(
            &registries.agent_templates,
            "agent-b".into(),
            agent_template("agent-b", "writing", "profile-claude"),
        );
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            registries,
            Arc::new(ResourcePluginRegistry::new()),
        ))
    }

    #[tokio::test]
    async fn query_filters_and_directories() {
        let ctx = make_ctx();

        let all = query(&ctx, None).unwrap();
        assert_eq!(all.len(), 2);

        let analytics = query_by_category(&ctx, "analytics").unwrap();
        assert_eq!(analytics.len(), 1);
        assert_eq!(analytics[0].id, "agent-a");

        let tagged = query_by_tags(&ctx, &["tag-b".to_string()]).unwrap();
        assert_eq!(tagged.len(), 2);

        let author = query_by_author(&ctx, "author-x").unwrap();
        assert_eq!(author.len(), 2);

        let profile = query_by_profile_type(&ctx, "profile-gpt").unwrap();
        assert_eq!(profile.len(), 1);
        assert_eq!(profile[0].id, "agent-a");
    }

    #[tokio::test]
    async fn featured_and_popular() {
        let ctx = make_ctx();
        ctx.template_usage.insert("agent-b".to_string(), 5);

        let featured = featured(&ctx, Some(10)).unwrap();
        assert_eq!(featured.len(), 2);
        assert_eq!(featured[0].id, "agent-b");

        let popular = popular_in_category(&ctx, "writing", Some(10)).unwrap();
        assert_eq!(popular.len(), 1);
        assert_eq!(popular[0].id, "agent-b");
    }
}
