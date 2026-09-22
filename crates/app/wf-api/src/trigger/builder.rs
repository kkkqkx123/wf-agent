//! Typed trigger template builder.
//!
//! Produces a typed trigger template artifact, validates it through the
//! `wf-config` trigger validator, and persists it into both the storage
//! adapter and the shared resource registry so the template is immediately
//! executable.

use wf_core::registry::Registry;
use wf_storage::adapter::base::BaseStorageAdapter;
use wf_types::trigger::{TriggerAction, TriggerCondition, TriggerTemplate};
use wf_types::Metadata;

use crate::infra::context::ApiContext;

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
    dispatch_mode: Option<wf_types::trigger::TriggerDispatchMode>,
    allow_multi_effect: Option<bool>,
    effect_order: Option<Vec<String>>,
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
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
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
    ///
    /// Priorities only take effect under the best-win dispatch mode, where
    /// every subscriber of one scope must declare a distinct priority.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Declare the dispatch mode for the event scope this template
    /// subscribes to. Absent means the default unique dispatch; set best-win
    /// to allow several subscribers with distinct explicit priorities.
    pub fn dispatch_mode(mut self, mode: wf_types::trigger::TriggerDispatchMode) -> Self {
        self.dispatch_mode = Some(mode);
        self
    }

    /// Opt into ordered multi-effect execution for one event (requires an
    /// explicit `effect_order`; default keeps single-winner single-execution).
    pub fn allow_multi_effect(mut self, allow: bool) -> Self {
        self.allow_multi_effect = Some(allow);
        self
    }

    /// Explicit execution order for multi-effect mode.
    pub fn effect_order(mut self, order: Vec<String>) -> Self {
        self.effect_order = Some(order);
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
            dispatch_mode: self.dispatch_mode,
            allow_multi_effect: self.allow_multi_effect,
            effect_order: self.effect_order,
            metadata: self.metadata,
            created_at: now,
            updated_at: now,
            create_checkpoint: self.create_checkpoint,
            checkpoint_description_template: self.checkpoint_description_template,
        };
        wf_config::processor::trigger::validate_trigger_template(&template)
            .map_err(crate::ApiError::from)?;
        Ok(template)
    }

    /// Build and validate with the shared validation context. Checks
    /// structural validity plus action reference closure (workflow, script,
    /// profile existence).
    pub fn build_with_validation(
        self,
        val_ctx: &wf_types::ValidationContext,
    ) -> crate::ApiResult<(TriggerTemplate, wf_types::ValidationResult)> {
        let template = self.build()?;
        let validator = crate::trigger::validation::TriggerValidator::new(val_ctx);
        let result = validator.validate(&template);
        Ok((template, result))
    }

    /// Build, validate and register the template (storage adapter + shared
    /// registry), so agent loops can reference it by name.
    ///
    /// Uses the unified registration entry
    /// (`wf_config::processor::trigger::validate_trigger_registration`):
    /// single-template shape plus the competition scopes of the merged
    /// registry set. A second subscriber of a unique-dispatch scope, or a
    /// best-win scope without distinct explicit priorities, rejects the
    /// registration.
    pub async fn register(self, ctx: &ApiContext) -> crate::ApiResult<()> {
        let template = self.build()?;
        let existing: Vec<TriggerTemplate> = ctx
            .registries
            .trigger_templates
            .list()
            .iter()
            .filter_map(|key| {
                ctx.registries
                    .trigger_templates
                    .get(key)
                    .map(|t| t.as_ref().clone())
            })
            .collect();
        let (validated, reports) = wf_config::processor::trigger::validate_trigger_registration(
            &existing,
            std::slice::from_ref(&template),
        );
        validated.map_err(crate::ApiError::from)?;
        for report in &reports {
            if report.incoming_names.is_empty() {
                tracing::warn!(
                    "pre-existing trigger scope violation without incoming subscriber: {}",
                    report.message,
                );
                continue;
            }
            if report.incoming_names.iter().any(|n| n == &template.name) {
                return Err(crate::ApiError::Validation(report.message.clone()));
            }
        }
        let condition_value = template
            .condition
            .as_ref()
            .and_then(|c| serde_json::to_value(c).ok());
        let metadata = wf_types::TriggerTemplateStorageMetadata {
            id: wf_types::Id::from(wf_common::generate_id()),
            name: template.name.clone(),
            trigger_type: crate::trigger::template::trigger_type_of(condition_value.as_ref())
                .to_string(),
            description: template.description.clone(),
            category: None,
            tags: None,
            enabled: template.enabled.unwrap_or(true),
            max_triggers: template.max_triggers,
            priority: template.priority,
            dispatch_mode: template.dispatch_mode,
            condition: condition_value,
            action_config: template
                .action
                .as_ref()
                .and_then(|a| serde_json::to_value(a).ok()),
            created_at: template.created_at,
            updated_at: template.updated_at,
        };
        ctx.storage.trigger_template.save(&metadata).await?;
        ctx.registries
            .register_trigger_template(template)
            .map_err(crate::ApiError::Conflict)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::trigger::TriggerCondition;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ))
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
}
