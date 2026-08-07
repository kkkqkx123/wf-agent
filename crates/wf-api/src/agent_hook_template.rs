use std::sync::Arc;

use serde::Serialize;

use wf_storage::adapter::agent_hook_template::AgentHookTemplateListOptions;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::AgentHookTemplateStorageMetadata;

use crate::context::ApiContext;
use crate::error::{ApiError, ApiResult};

/// Agent hook template filter (TS `AgentHookTemplateFilter`).
#[derive(Debug, Clone, Default)]
pub struct AgentHookTemplateFilter {
    pub hook_type: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub name: Option<String>,
}

/// Digest of an agent hook template (TS `AgentHookTemplateSummary`).
#[derive(Debug, Clone, Serialize)]
pub struct AgentHookTemplateSummary {
    pub id: String,
    pub name: String,
    pub hook_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    pub updated_at: i64,
}

/// Agent hook template registry (TS `AgentHookTemplateRegistryAPI` counterpart).
pub struct AgentHookTemplateRegistryApi {
    ctx: Arc<ApiContext>,
}

impl AgentHookTemplateRegistryApi {
    pub fn new(ctx: Arc<ApiContext>) -> Self {
        Self { ctx }
    }

    /// Query hook templates with an optional filter.
    pub async fn query(
        &self,
        filter: Option<&AgentHookTemplateFilter>,
    ) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        let options = filter.map(|f| AgentHookTemplateListOptions {
            offset: None,
            limit: None,
            hook_type_filter: f.hook_type.clone(),
            name_filter: f.name.clone(),
            category_filter: f.category.clone(),
        });
        let mut templates = self.ctx.storage.agent_hook_template.list(options).await?;
        if let Some(filter) = filter {
            if let Some(tags) = &filter.tags {
                templates.retain(|t| {
                    tags.is_empty()
                        || t.tags
                            .as_ref()
                            .map(|existing| tags.iter().any(|tag| existing.contains(tag)))
                            .unwrap_or(false)
                });
            }
        }
        Ok(templates)
    }

    pub async fn query_by_hook_type(&self, hook_type: &str) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        self.query(Some(&AgentHookTemplateFilter {
            hook_type: Some(hook_type.to_string()),
            ..AgentHookTemplateFilter::default()
        }))
        .await
    }

    pub async fn query_by_category(&self, category: &str) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        self.query(Some(&AgentHookTemplateFilter {
            category: Some(category.to_string()),
            ..AgentHookTemplateFilter::default()
        }))
        .await
    }

    pub async fn query_by_tags(&self, tags: &[String]) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        self.query(Some(&AgentHookTemplateFilter {
            tags: Some(tags.to_vec()),
            ..AgentHookTemplateFilter::default()
        }))
        .await
    }

    /// Templates applicable to a hook type (alias for `query_by_hook_type`).
    pub async fn templates_for_hook(&self, hook_type: &str) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        self.query_by_hook_type(hook_type).await
    }

    /// Template summaries, optionally filtered.
    pub async fn summaries(
        &self,
        filter: Option<&AgentHookTemplateFilter>,
    ) -> ApiResult<Vec<AgentHookTemplateSummary>> {
        Ok(self
            .query(filter)
            .await?
            .into_iter()
            .map(|t| AgentHookTemplateSummary {
                id: t.id.to_string(),
                name: t.name,
                hook_type: t.hook_type,
                description: t.description,
                category: t.category,
                tags: t.tags,
                updated_at: t.updated_at,
            })
            .collect())
    }

    /// Keyword search over template names / descriptions.
    pub async fn search(&self, keyword: &str) -> ApiResult<Vec<AgentHookTemplateStorageMetadata>> {
        let keyword = keyword.trim().to_lowercase();
        let all = self.ctx.storage.agent_hook_template.list(None).await?;
        Ok(all
            .into_iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&keyword)
                    || t.description
                        .as_deref()
                        .map(|d| d.to_lowercase().contains(&keyword))
                        .unwrap_or(false)
            })
            .collect())
    }

    /// Validate a hook template: the hook type must be non-empty.
    pub fn validate(&self, template: &AgentHookTemplateStorageMetadata) -> ApiResult<()> {
        if template.name.trim().is_empty() {
            return Err(ApiError::Validation("hook template name is empty".into()));
        }
        if template.hook_type.trim().is_empty() {
            return Err(ApiError::Validation("hook template hook_type is empty".into()));
        }
        Ok(())
    }

    /// Register or overwrite a hook template.
    pub async fn save(&self, template: &AgentHookTemplateStorageMetadata) -> ApiResult<()> {
        self.validate(template)?;
        self.ctx.storage.agent_hook_template.save(template).await?;
        Ok(())
    }

    pub async fn get(&self, id: &str) -> ApiResult<AgentHookTemplateStorageMetadata> {
        self.ctx
            .storage
            .agent_hook_template
            .load(id)
            .await?
            .ok_or_else(|| ApiError::not_found("agent_hook_template", id))
    }

    pub async fn delete(&self, id: &str) -> ApiResult<bool> {
        self.ctx
            .storage
            .agent_hook_template
            .delete(id)
            .await
            .map_err(Into::into)
    }

    /// Export a template by name as a JSON string.
    pub async fn export_template(&self, name: &str) -> ApiResult<String> {
        let template = self
            .ctx
            .storage
            .agent_hook_template
            .list(None)
            .await?
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| ApiError::not_found("agent_hook_template", name))?;
        serde_json::to_string_pretty(&template).map_err(Into::into)
    }

    /// Import a template from a JSON string; returns the imported id.
    pub async fn import_template(&self, json: &str) -> ApiResult<String> {
        let template: AgentHookTemplateStorageMetadata =
            serde_json::from_str(json).map_err(ApiError::from)?;
        self.save(&template).await?;
        Ok(template.id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn make_template(id: &str, hook_type: &str) -> AgentHookTemplateStorageMetadata {
        AgentHookTemplateStorageMetadata {
            id: id.into(),
            name: format!("hook-{id}"),
            hook_type: hook_type.into(),
            description: Some(format!("desc {id}")),
            category: Some("lifecycle".into()),
            tags: Some(vec!["tag-h".into()]),
            hook_config: None,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    #[tokio::test]
    async fn query_by_hook_type_category_tags_and_search() {
        let ctx = make_ctx();
        let api = AgentHookTemplateRegistryApi::new(ctx.clone());
        api.save(&make_template("h1", "before_iteration")).await.unwrap();
        api.save(&make_template("h2", "after_tool_call")).await.unwrap();

        let all = api.query(None).await.unwrap();
        assert_eq!(all.len(), 2);

        let before = api.query_by_hook_type("before_iteration").await.unwrap();
        assert_eq!(before.len(), 1);

        let lifecycle = api.query_by_category("lifecycle").await.unwrap();
        assert_eq!(lifecycle.len(), 2);

        let tagged = api.query_by_tags(&["tag-h".to_string()]).await.unwrap();
        assert_eq!(tagged.len(), 2);

        let for_hook = api.templates_for_hook("before_iteration").await.unwrap();
        assert_eq!(for_hook.len(), 1);

        let matches = api.search("desc h2").await.unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[tokio::test]
    async fn validate_and_summaries_and_import_export() {
        let ctx = make_ctx();
        let api = AgentHookTemplateRegistryApi::new(ctx.clone());

        let invalid = AgentHookTemplateStorageMetadata {
            name: "".into(),
            ..make_template("bad", "before_iteration")
        };
        let err = api.save(&invalid).await.unwrap_err();
        assert!(matches!(err, ApiError::Validation(_)));

        api.save(&make_template("h1", "before_iteration")).await.unwrap();
        let summaries = api.summaries(None).await.unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].hook_type, "before_iteration");

        let exported = api.export_template("hook-h1").await.unwrap();
        let imported_id = api.import_template(&exported).await.unwrap();
        assert_eq!(imported_id, "h1");

        assert!(api.delete("h1").await.unwrap());
    }
}
