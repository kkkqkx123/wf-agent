use std::sync::Arc;

use wf_core::registry::Registry;
use wf_types::agent::AgentTemplate;

use crate::context::ApiContext;
use crate::error::ApiResult;
use crate::template_library::{TemplateFilter, TemplateLibraryApi, TemplateSummary};

/// Agent template filter (TS `AgentTemplateFilter`).
#[derive(Debug, Clone, Default)]
pub struct AgentTemplateFilter {
    pub name: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
    pub profile_type: Option<String>,
}

/// Agent template registry (TS `AgentTemplateRegistryAPI` counterpart).
///
/// Backed by the shared `wf-resource` agent template registry (predefined +
/// custom), the same source the workflow/agent template library reads.
pub struct AgentTemplateRegistryApi {
    ctx: Arc<ApiContext>,
}

impl AgentTemplateRegistryApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Query agent templates with an optional filter.
    pub fn query(&self, filter: Option<&AgentTemplateFilter>) -> ApiResult<Vec<AgentTemplate>> {
        let all = self.list_agent_templates()?;
        Ok(all
            .into_iter()
            .filter(|t| {
                let Some(filter) = filter else {
                    return true;
                };
                if let Some(name) = &filter.name {
                    if !t.name.to_lowercase().contains(&name.to_lowercase()) {
                        return false;
                    }
                }
                let category = t
                    .template_category
                    .clone()
                    .or_else(|| t.definition.metadata.as_ref().and_then(|m| m.category.clone()));
                if let Some(category_filter) = &filter.category {
                    if category.as_deref() != Some(category_filter.as_str()) {
                        return false;
                    }
                }
                if let Some(tags) = &filter.tags {
                    if !tags.is_empty() {
                        let existing = t
                            .template_tags
                            .clone()
                            .or_else(|| {
                                t.definition
                                    .metadata
                                    .as_ref()
                                    .and_then(|m| m.tags.clone())
                            })
                            .unwrap_or_default();
                        if !tags.iter().any(|tag| existing.contains(tag)) {
                            return false;
                        }
                    }
                }
                if let Some(author) = &filter.author {
                    if t.definition.metadata.as_ref().and_then(|m| m.author.as_ref())
                        != Some(author)
                    {
                        return false;
                    }
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

    pub fn query_by_category(&self, category: &str) -> ApiResult<Vec<AgentTemplate>> {
        self.query(Some(&AgentTemplateFilter {
            category: Some(category.to_string()),
            ..AgentTemplateFilter::default()
        }))
    }

    pub fn query_by_tags(&self, tags: &[String]) -> ApiResult<Vec<AgentTemplate>> {
        self.query(Some(&AgentTemplateFilter {
            tags: Some(tags.to_vec()),
            ..AgentTemplateFilter::default()
        }))
    }

    pub fn query_by_author(&self, author: &str) -> ApiResult<Vec<AgentTemplate>> {
        self.query(Some(&AgentTemplateFilter {
            author: Some(author.to_string()),
            ..AgentTemplateFilter::default()
        }))
    }

    pub fn query_by_profile_type(&self, profile_type: &str) -> ApiResult<Vec<AgentTemplate>> {
        self.query(Some(&AgentTemplateFilter {
            profile_type: Some(profile_type.to_string()),
            ..AgentTemplateFilter::default()
        }))
    }

    /// Featured templates: public + enabled, most used first.
    pub fn featured(&self, limit: usize) -> ApiResult<Vec<TemplateSummary>> {
        let library = TemplateLibraryApi::new(self.ctx.clone());
        Ok(library
            .featured(limit)?
            .into_iter()
            .filter(|t| t.kind == "agent")
            .collect())
    }

    /// Popular templates within a category, most used first.
    pub fn popular_in_category(&self, category: &str, limit: usize) -> ApiResult<Vec<TemplateSummary>> {
        let library = TemplateLibraryApi::new(self.ctx.clone());
        Ok(library
            .popular_in_category(category, limit)?
            .into_iter()
            .filter(|t| t.kind == "agent")
            .collect())
    }

    /// Uniform summaries across the matching agent templates.
    pub fn summaries(&self, filter: Option<&AgentTemplateFilter>) -> ApiResult<Vec<TemplateSummary>> {
        let library = TemplateLibraryApi::new(self.ctx.clone());
        let ids: Vec<String> = self
            .query(filter)?
            .into_iter()
            .map(|t| t.id.to_string())
            .collect();
        Ok(library
            .query(&TemplateFilter {
                kind: Some(crate::template_library::TemplateKind::Agent),
                ..TemplateFilter::default()
            })?
            .into_iter()
            .filter(|s| ids.contains(&s.id))
            .collect())
    }

    fn list_agent_templates(&self) -> ApiResult<Vec<AgentTemplate>> {
        let keys = self.ctx.registries.agent_templates.list();
        let mut templates = Vec::new();
        for key in keys {
            if let Some(template) = self.ctx.registries.agent_templates.get(&key) {
                templates.push(template.as_ref().clone());
            }
        }
        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::{register_item, Registries};
    use wf_resource::starter::BundleRegistry;
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
                    initial_messages: None,
                    available_tools: None,
                    stream: None,
                    tool_call_format: None,
                    hooks: None,
                    triggers: None,
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
        let registries = Arc::new(Registries::new());
        register_item(
            &registries.agent_templates,
            "agent-a".into(),
            agent_template("agent-a", "analytics", "profile-gpt"),
            true,
        );
        register_item(
            &registries.agent_templates,
            "agent-b".into(),
            agent_template("agent-b", "writing", "profile-claude"),
            true,
        );
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            registries,
            Arc::new(BundleRegistry::new()),
        ))
    }

    #[test]
    fn query_filters_and_directories() {
        let ctx = make_ctx();
        let api = AgentTemplateRegistryApi::new(ctx);

        let all = api.query(None).unwrap();
        assert_eq!(all.len(), 2);

        let analytics = api.query_by_category("analytics").unwrap();
        assert_eq!(analytics.len(), 1);
        assert_eq!(analytics[0].id, "agent-a");

        let tagged = api.query_by_tags(&["tag-b".to_string()]).unwrap();
        assert_eq!(tagged.len(), 2);

        let author = api.query_by_author("author-x").unwrap();
        assert_eq!(author.len(), 2);

        let profile = api.query_by_profile_type("profile-gpt").unwrap();
        assert_eq!(profile.len(), 1);
        assert_eq!(profile[0].id, "agent-a");
    }

    #[test]
    fn featured_and_popular() {
        let ctx = make_ctx();
        let api = AgentTemplateRegistryApi::new(ctx);
        api.ctx.template_usage.insert("agent-b".to_string(), 5);

        let featured = api.featured(10).unwrap();
        assert_eq!(featured.len(), 2);
        assert_eq!(featured[0].id, "agent-b");

        let popular = api.popular_in_category("writing", 10).unwrap();
        assert_eq!(popular.len(), 1);
        assert_eq!(popular[0].id, "agent-b");
    }
}
