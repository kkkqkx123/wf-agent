//! Typed template builders.
//!
//! Mirrors the TS `NodeTemplateBuilder` / `TriggerTemplateBuilder` /
//! `HookTemplateBuilder` (`packages/sdk/api/workflow/builders`): each builder
//! produces a typed template artifact, validates it through the `wf-config`
//! template validators, and persists it into both the storage adapter and the
//! shared resource registry so the template is immediately executable.

use std::sync::Arc;

use serde_json::Value;

use wf_core::registry::MutableRegistry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::workflow::hook_template::{HookTemplate, WorkflowHookType};
use wf_types::workflow::node_template::NodeTemplate;
use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_types::Metadata;

use crate::context::ApiContext;

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
            .node_templates
            .register(template.id.clone(), Arc::new(template))
            .map_err(|e| crate::ApiError::Conflict(e.to_string()))?;
        Ok(())
    }
}

/// Consuming builder for [`TriggerTemplate`].
#[derive(Debug)]
pub struct TriggerTemplateBuilder {
    name: String,
    description: Option<String>,
    condition: Option<TriggerCondition>,
    action: Option<TriggerAction>,
    enabled: Option<bool>,
    max_triggers: Option<u32>,
    priority: Option<i32>,
    metadata: Option<Metadata>,
    create_checkpoint: Option<bool>,
    checkpoint_description_template: Option<String>,
}

impl TriggerTemplateBuilder {
    /// Start building a trigger template. A condition (or action) is
    /// validated at `build` time.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            condition: None,
            action: None,
            enabled: None,
            max_triggers: None,
            priority: None,
            metadata: None,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    /// Set the template description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the matching condition.
    pub fn condition(mut self, condition: TriggerCondition) -> Self {
        self.condition = Some(condition);
        self
    }

    /// Set the trigger action.
    pub fn action(mut self, action: TriggerAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Enable or disable the template.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = Some(enabled);
        self
    }

    /// Cap the number of times the trigger fires.
    pub fn max_triggers(mut self, max_triggers: u32) -> Self {
        self.max_triggers = Some(max_triggers);
        self
    }

    /// Set template priority (higher wins when multiple templates match).
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Create a checkpoint when the trigger fires.
    pub fn create_checkpoint(mut self) -> Self {
        self.create_checkpoint = Some(true);
        self
    }

    /// Validate the template (name required, condition/action closure) and
    /// build it.
    pub fn build(self) -> crate::ApiResult<TriggerTemplate> {
        let now = wf_common::now();
        let template = TriggerTemplate {
            name: self.name,
            description: self.description,
            condition: self.condition,
            action: self.action,
            enabled: self.enabled,
            max_triggers: self.max_triggers,
            priority: self.priority,
            metadata: self.metadata,
            created_at: now,
            updated_at: now,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description_template: self.checkpoint_description_template,
        };
        wf_config::processor::trigger::validate_trigger_template(&template).map_err(crate::ApiError::from)?;
        Ok(template)
    }

    /// Build, validate and register the template (storage adapter + shared
    /// registry), so agent loops can reference it by name.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let template = self.build()?;
        let metadata = wf_types::TriggerTemplateStorageMetadata {
            id: wf_types::Id::from(wf_common::generate_id()),
            name: template.name.clone(),
            trigger_type: trigger_type_string(&template),
            description: template.description.clone(),
            category: None,
            tags: None,
            enabled: template.enabled.unwrap_or(true),
            max_triggers: template.max_triggers,
            priority: template.priority,
            condition: template
                .condition
                .as_ref()
                .and_then(|c| serde_json::to_value(c).ok()),
            action_config: template
                .action
                .as_ref()
                .and_then(|a| serde_json::to_value(a).ok()),
            created_at: template.created_at,
            updated_at: template.updated_at,
        };
        ctx.storage.trigger_template.save(&metadata).await?;
        ctx.registries
            .trigger_templates
            .register(template.name.clone(), Arc::new(template))
            .map_err(|e| crate::ApiError::Conflict(e.to_string()))?;
        Ok(())
    }
}

/// Classify a trigger template for its persisted metadata (`event` when a
/// condition matches on event type, `condition` for expression conditions,
/// `schedule` otherwise).
fn trigger_type_string(template: &TriggerTemplate) -> String {
    match &template.condition {
        Some(condition) if condition.event_type.is_empty() => "condition".to_string(),
        Some(_) => "event".to_string(),
        None => "schedule".to_string(),
    }
}

/// Consuming builder for [`HookTemplate`].
#[derive(Debug)]
pub struct HookTemplateBuilder {
    id: String,
    name: String,
    description: String,
    hook_type: WorkflowHookType,
    default_config: Option<Value>,
}

impl HookTemplateBuilder {
    /// Start building a hook template for the given workflow hook type.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        hook_type: WorkflowHookType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            hook_type,
            description: String::new(),
            default_config: None,
        }
    }

    /// Set the template description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the default hook config carried by the template.
    pub fn default_config(mut self, config: Value) -> Self {
        self.default_config = Some(config);
        self
    }

    /// Validate the template (name required) and build it.
    pub fn build(self) -> crate::ApiResult<HookTemplate> {
        let template = HookTemplate {
            id: self.id,
            name: self.name,
            description: self.description,
            hook_type: self.hook_type,
            default_config: self.default_config,
        };
        wf_config::processor::hook::validate_hook_template(&template).map_err(crate::ApiError::from)?;
        Ok(template)
    }

    /// Build, validate and register the template (storage adapter + shared
    /// registry), so workflow hooks can reference it.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let template = self.build()?;
        let metadata = wf_types::HookTemplateStorageMetadata {
            id: template.id.clone(),
            name: template.name.clone(),
            hook_type: serde_json::to_string(&template.hook_type)
                .map(|t| t.trim_matches('"').to_string())
                .unwrap_or_else(|_| format!("{:?}", template.hook_type)),
            description: (!template.description.is_empty()).then_some(template.description.clone()),
            created_at: wf_common::now(),
            updated_at: wf_common::now(),
        };
        ctx.storage.hook_template.save(&metadata).await?;
        ctx.registries
            .hook_templates
            .register(template.id.clone(), Arc::new(template))
            .map_err(|e| crate::ApiError::Conflict(e.to_string()))?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_types::trigger::TriggerCondition;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
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
        let err = NodeTemplateBuilder::new("nt-2", "", "LLM").build().unwrap_err();
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

    #[test]
    fn trigger_template_build_and_validate() {
        let template = TriggerTemplateBuilder::new("on-high-risk")
            .condition(TriggerCondition {
                event_type: "TOOL_APPROVAL_REQUESTED".into(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            })
            .action(wf_types::trigger::TriggerAction::PauseWorkflowExecution {})
            .max_triggers(3)
            .build()
            .expect("trigger template must build");
        assert_eq!(template.name, "on-high-risk");
        assert_eq!(template.max_triggers, Some(3));
    }

    #[tokio::test]
    async fn trigger_template_register_persists_and_indexes() {
        let ctx = make_ctx();
        TriggerTemplateBuilder::new("tt-reg")
            .condition(TriggerCondition {
                event_type: "AGENT_STARTED".into(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            })
            .register(&ctx)
            .await
            .expect("register must succeed");
        assert!(ctx.registries.trigger_templates.has("tt-reg"));
        let listed = ctx.storage.trigger_template.list(None).await.unwrap();
        assert_eq!(listed.len(), 1);
    }

    #[test]
    fn hook_template_build_and_validate() {
        let template = HookTemplateBuilder::new(
            "ht-1",
            "Audit Hook",
            WorkflowHookType::AfterNode,
        )
        .default_config(serde_json::json!({"event_name": "node-audit"}))
        .build()
        .expect("hook template must build");
        assert_eq!(template.hook_type, WorkflowHookType::AfterNode);
        assert_eq!(template.default_config.unwrap()["event_name"], "node-audit");
    }

    #[tokio::test]
    async fn hook_template_register_persists_and_indexes() {
        let ctx = make_ctx();
        HookTemplateBuilder::new("ht-reg", "Reg Hook", WorkflowHookType::BeforeNode)
            .register(&ctx)
            .await
            .expect("register must succeed");
        assert!(ctx.registries.hook_templates.has("ht-reg"));
        let loaded = ctx.storage.hook_template.load("ht-reg").await.unwrap();
        assert_eq!(loaded.unwrap().hook_type, "BEFORE_NODE");
    }
}
