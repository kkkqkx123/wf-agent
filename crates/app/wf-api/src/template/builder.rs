//! Typed node template builder.
//!
//! Produces a typed node template artifact, validates it through the
//! `wf-config` template validator, and persists it into both the storage
//! adapter and the shared resource registry so the template is immediately
//! executable.

use serde_json::Value;

use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow::node_template::NodeTemplate;

use crate::infra::context::ApiContext;

/// Consuming builder for [`NodeTemplate`].
#[derive(Debug)]
pub struct NodeTemplateBuilder {
    id: String,
    name: String,
    description: String,
    node_type: String,
    default_config: Option<Value>,
}

impl NodeTemplateBuilder {
    /// Start building a node template.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        node_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            node_type: node_type.into(),
            description: String::new(),
            default_config: None,
        }
    }

    /// Set the template description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the default node config carried by the template.
    pub fn default_config(mut self, config: Value) -> Self {
        self.default_config = Some(config);
        self
    }

    /// Validate the template (name/node type required) and build it.
    pub fn build(self) -> crate::ApiResult<NodeTemplate> {
        let template = NodeTemplate {
            id: self.id,
            name: self.name,
            description: self.description,
            node_type: self.node_type,
            default_config: self.default_config,
        };
        wf_config::processor::node_template::validate_node_template(&template)
            .map_err(crate::ApiError::from)?;
        Ok(template)
    }

    /// Build, validate and register the template (storage adapter + shared
    /// registry), so workflow node configs can reference it.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let template = self.build()?;
        let metadata = wf_types::NodeTemplateStorageMetadata {
            id: template.id.clone(),
            name: template.name.clone(),
            node_type: template.node_type.clone(),
            description: (!template.description.is_empty()).then_some(template.description.clone()),
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        };
        ctx.storage.node_template.save(&metadata).await?;
        ctx.registries
            .register_node_template(template)
            .map_err(crate::ApiError::Conflict)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
    }

    #[test]
    fn node_template_build_and_validate() {
        let template = NodeTemplateBuilder::new("nt-1", "Code Template", "LLM")
            .description("reusable llm node")
            .default_config(serde_json::json!({"profile_id": "mock"}))
            .build()
            .expect("node template must build");
        assert_eq!(template.node_type, "LLM");
        assert_eq!(template.default_config.unwrap()["profile_id"], "mock");
    }

    #[test]
    fn node_template_builder_rejects_empty_name() {
        let err = NodeTemplateBuilder::new("nt-2", "", "LLM")
            .build()
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[tokio::test]
    async fn node_template_register_persists_and_indexes() {
        let ctx = make_ctx();
        NodeTemplateBuilder::new("nt-reg", "Reg Template", "LLM")
            .register(&ctx)
            .await
            .expect("register must succeed");
        assert!(ctx.registries.node_templates.has("nt-reg"));
        let loaded = ctx.storage.node_template.load("nt-reg").await.unwrap();
        assert_eq!(loaded.unwrap().name, "Reg Template");
    }
}
