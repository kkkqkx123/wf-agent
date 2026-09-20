//! Declarative resource bundles assembled from config.
//!
//! A `ResourcePlugin` here is a config-to-bundle assembler, not a
//! `wf-plugin::Plugin`. It has no isolation, no manifest, and no execution
//! hooks. This module only builds `ResourceBundle` values and lands them
//! into `ResourceRegistries` / `ToolRegistry` through `install_bundle` /
//! `uninstall_bundle`. Activation state is owned by the plugin engine
//! (`wf-runtime` bridges assemblers into it); builds without the plugin
//! engine assemble requested bundles directly.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_core::Registry;
use wf_tools::registry::ToolRegistry;
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::{SystemPromptFragment, Template};

use crate::registry::{
    register_fragment, register_item_skip, register_item_strict, register_template,
    ResourceRegistries,
};
use crate::result::Summary;

#[derive(Debug, Clone)]
pub struct ResourceBundle {
    pub workflows: Vec<WorkflowTemplate>,
    pub tools: Vec<ToolDef>,
    pub triggers: Vec<TriggerTemplate>,
    pub prompts: Vec<Template>,
    pub fragments: Vec<SystemPromptFragment>,
    pub tool_descriptions: Vec<ToolDescriptionData>,
    pub node_templates: Vec<NodeTemplate>,
    pub agent_templates: Vec<AgentTemplate>,
}

impl ResourceBundle {
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
            tools: Vec::new(),
            triggers: Vec::new(),
            prompts: Vec::new(),
            fragments: Vec::new(),
            tool_descriptions: Vec::new(),
            node_templates: Vec::new(),
            agent_templates: Vec::new(),
        }
    }
}

impl Default for ResourceBundle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePluginConfigFieldType {
    String,
    Number,
    Boolean,
    Expression,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePluginConfigField {
    pub r#type: ResourcePluginConfigFieldType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_functions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourcePluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configurable: Option<HashMap<String, ResourcePluginConfigField>>,
}

pub trait ResourcePlugin: Send + Sync {
    fn metadata(&self) -> ResourcePluginMetadata;
    fn assemble(&self, config: &Value) -> Result<ResourceBundle, String>;

    fn on_before_assemble(&self, _config: &Value) -> Result<(), String> {
        Ok(())
    }
    fn on_after_install(&self, _bundle: &ResourceBundle) -> Result<(), String> {
        Ok(())
    }
    fn on_before_uninstall(&self) -> Result<(), String> {
        Ok(())
    }
    fn on_after_uninstall(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Single landing point for a `ResourceBundle`.
///
/// Both direct installation (builds without the plugin engine) and the
/// plugin-engine bridge land bundles through this helper so validation and
/// skip-existing semantics stay identical. Prompts, fragments, and triggers
/// go through their validated registration helpers; the remaining kinds
/// have no extra validation and use strict/skip item registration.
pub fn install_bundle(
    registries: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    bundle: &ResourceBundle,
    skip_if_exists: bool,
) -> Summary {
    let mut total = Summary::new();

    for wf in &bundle.workflows {
        let key = wf.id.clone();
        total.merge(if skip_if_exists {
            register_item_skip(&registries.workflows, key, wf.clone())
        } else {
            register_item_strict(&registries.workflows, key, wf.clone())
        });
    }
    for tool in &bundle.tools {
        let key = tool.id.clone();
        if skip_if_exists && tool_registry.has(&key) {
            total.merge(Summary::ok(&key));
            continue;
        }
        tool_registry.register_tool(tool.clone());
        total.merge(Summary::ok(&key));
    }
    total.merge(crate::registry::register_trigger_candidates(
        registries,
        bundle.triggers.clone(),
        skip_if_exists,
    ));
    for prompt in &bundle.prompts {
        total.merge(register_template(registries, prompt.clone(), skip_if_exists));
    }
    for fragment in &bundle.fragments {
        total.merge(register_fragment(
            registries,
            fragment.clone(),
            skip_if_exists,
        ));
    }
    for description in &bundle.tool_descriptions {
        let key = description.id.clone();
        total.merge(if skip_if_exists {
            register_item_skip(&registries.tool_descriptions, key, description.clone())
        } else {
            register_item_strict(&registries.tool_descriptions, key, description.clone())
        });
    }
    for node_tmpl in &bundle.node_templates {
        let key = node_tmpl.id.clone();
        total.merge(if skip_if_exists {
            register_item_skip(&registries.node_templates, key, node_tmpl.clone())
        } else {
            register_item_strict(&registries.node_templates, key, node_tmpl.clone())
        });
    }
    for agent_tmpl in &bundle.agent_templates {
        let key = agent_tmpl.id.clone();
        total.merge(if skip_if_exists {
            register_item_skip(&registries.agent_templates, key, agent_tmpl.clone())
        } else {
            register_item_strict(&registries.agent_templates, key, agent_tmpl.clone())
        });
    }

    total
}

/// Symmetric teardown of `install_bundle` through the controlled
/// `ResourceRegistries` removal entry points.
pub fn uninstall_bundle(
    registries: &ResourceRegistries,
    tool_registry: &ToolRegistry,
    bundle: &ResourceBundle,
) -> Summary {
    let mut total = Summary::new();

    for wf in &bundle.workflows {
        if registries.remove_workflow_template(&wf.id) {
            total.merge(Summary::ok(&wf.id));
        }
    }
    for tool in &bundle.tools {
        if tool_registry.remove_tool(&tool.id).is_some() {
            total.merge(Summary::ok(&tool.id));
        }
    }
    for trigger in &bundle.triggers {
        if registries.remove_trigger_template(&trigger.name) {
            total.merge(Summary::ok(&trigger.name));
        }
    }
    for prompt in &bundle.prompts {
        if registries.remove_prompt_template(&prompt.id) {
            total.merge(Summary::ok(&prompt.id));
        }
    }
    for fragment in &bundle.fragments {
        if registries.remove_fragment(&fragment.id) {
            total.merge(Summary::ok(&fragment.id));
        }
    }
    for description in &bundle.tool_descriptions {
        if registries.remove_tool_description(&description.id) {
            total.merge(Summary::ok(&description.id));
        }
    }
    for node_tmpl in &bundle.node_templates {
        if registries.remove_node_template(&node_tmpl.id) {
            total.merge(Summary::ok(&node_tmpl.id));
        }
    }
    for agent_tmpl in &bundle.agent_templates {
        if registries.remove_agent_template(&agent_tmpl.id) {
            total.merge(Summary::ok(&agent_tmpl.id));
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_core::registry::Registry;

    fn bundle() -> ResourceBundle {
        // Fixed bundle (not `builtin_agent_templates()`): the test must
        // not depend on how many built-in templates exist.
        let mut bundle = ResourceBundle::new();
        bundle.agent_templates = vec![
            crate::predefined::agent_templates::goal_review_executor(),
            crate::predefined::agent_templates::goal_review_reviewer(),
        ];
        bundle.prompts.push(Template {
            id: "test.prompt".into(),
            name: "Test Prompt".into(),
            description: Some("Test".into()),
            category: "system".into(),
            content: "hello".into(),
            variables: None,
            fragments: None,
        });
        bundle
    }

    fn trigger(name: &str) -> TriggerTemplate {
        TriggerTemplate {
            name: name.to_string(),
            description: None,
            condition: Some(wf_types::trigger::TriggerCondition {
                event_type: "NODE_COMPLETED".to_string(),
                event_name: None,
                condition: None,
                metadata: None,
                metadata_exists: None,
                execution_prefix: None,
            }),
            action: Some(wf_types::trigger::TriggerAction::StopWorkflowExecution {}),
            enabled: Some(true),
            max_triggers: None,
            priority: None,
            dispatch_mode: None,
            allow_multi_effect: None,
            effect_order: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            create_checkpoint: None,
            checkpoint_description_template: None,
        }
    }

    #[test]
    fn install_and_uninstall_roundtrip() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let bundle = bundle();

        let summary = install_bundle(&regs, &tool_registry, &bundle, true);
        assert!(summary.is_ok());

        // Every bundle item is registered.
        assert!(regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(regs.templates.has("test.prompt"));

        // Re-installing with skip_if_exists stays consistent.
        let again = install_bundle(&regs, &tool_registry, &bundle, true);
        assert!(again.is_ok());

        // Uninstall removes every installed item.
        let removed = uninstall_bundle(&regs, &tool_registry, &bundle);
        assert_eq!(removed.succeeded.len(), 3);
        assert!(!regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(!regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(!regs.templates.has("test.prompt"));

        // Uninstalling twice is a no-op.
        let again = uninstall_bundle(&regs, &tool_registry, &bundle);
        assert!(again.succeeded.is_empty());
    }

    #[test]
    fn install_strict_reports_duplicates() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let bundle = bundle();

        let first = install_bundle(&regs, &tool_registry, &bundle, false);
        assert!(first.is_ok());
        let second = install_bundle(&regs, &tool_registry, &bundle, false);
        assert!(!second.is_ok());
        assert_eq!(second.failed.len(), 3);
    }

    #[test]
    fn install_rejects_invalid_prompt() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let mut bundle = ResourceBundle::new();
        bundle.prompts.push(Template {
            id: "test.bad".into(),
            name: "Bad".into(),
            description: None,
            category: "nope".into(),
            content: "hello".into(),
            variables: None,
            fragments: None,
        });

        let summary = install_bundle(&regs, &tool_registry, &bundle, false);
        assert!(summary.failed.iter().any(|f| f.id == "test.bad"));
        assert!(!regs.templates.has("test.bad"));
    }

    #[test]
    fn install_rejects_conflicting_triggers() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let mut bundle = ResourceBundle::new();
        bundle.triggers = vec![trigger("plugin-a"), trigger("plugin-b")];

        let summary = install_bundle(&regs, &tool_registry, &bundle, false);
        assert!(summary.failed.iter().any(|f| f.id == "plugin-a"));
        assert!(summary.failed.iter().any(|f| f.id == "plugin-b"));
        assert!(!regs.trigger_templates.has("plugin-a"));
        assert!(!regs.trigger_templates.has("plugin-b"));
    }
}
