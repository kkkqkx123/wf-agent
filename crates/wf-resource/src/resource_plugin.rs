use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_tools::registry::ToolRegistry;
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{NodeTemplate, WorkflowTemplate};
use wf_types::Template;

use crate::registry::{register_item_skip, register_item_strict, ResourceRegistries};
use crate::result::Summary;

#[derive(Debug, Clone)]
pub struct ResourceBundle {
    pub workflows: Vec<WorkflowTemplate>,
    pub tools: Vec<ToolDef>,
    pub triggers: Vec<TriggerTemplate>,
    pub prompts: Vec<Template>,
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

pub struct ResourcePluginRegistry {
    resource_plugins: ConcurrentRegistry<Box<dyn ResourcePlugin>>,
    active_bundles: ConcurrentRegistry<ResourceBundle>,
}

impl ResourcePluginRegistry {
    pub fn new() -> Self {
        Self {
            resource_plugins: ConcurrentRegistry::new(),
            active_bundles: ConcurrentRegistry::new(),
        }
    }

    pub fn register(&self, plugin: Box<dyn ResourcePlugin>) -> Result<(), String> {
        let id = plugin.metadata().id;
        let item = Arc::new(plugin);
        self.resource_plugins
            .register(id.clone(), item)
            .map_err(|e| e.to_string())
    }

    pub fn unregister(&self, id: &str) {
        self.resource_plugins.unregister(id);
    }

    pub fn get(&self, id: &str) -> Option<Arc<Box<dyn ResourcePlugin>>> {
        self.resource_plugins.get(id)
    }

    pub fn list(&self) -> Vec<String> {
        self.resource_plugins.list()
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.active_bundles.has(id)
    }

    pub fn activate(
        &self,
        id: &str,
        config: &Value,
        registries: &ResourceRegistries,
        tool_registry: &ToolRegistry,
        skip_if_exists: bool,
    ) -> Result<Summary, String> {
        let plugin = self
            .resource_plugins
            .get(id)
            .ok_or_else(|| format!("ResourcePlugin \"{id}\" not found"))?;

        plugin.on_before_assemble(config)?;

        let bundle = plugin.assemble(config)?;
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
        for trigger in &bundle.triggers {
            let key = trigger.name.clone();
            total.merge(if skip_if_exists {
                register_item_skip(&registries.trigger_templates, key, trigger.clone())
            } else {
                register_item_strict(&registries.trigger_templates, key, trigger.clone())
            });
        }
        for prompt in &bundle.prompts {
            let key = prompt.id.clone();
            total.merge(if skip_if_exists {
                register_item_skip(&registries.templates, key, prompt.clone())
            } else {
                register_item_strict(&registries.templates, key, prompt.clone())
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

        plugin.on_after_install(&bundle)?;

        // Re-activation replaces the tracked bundle.
        self.active_bundles.unregister(id);
        self.active_bundles
            .register(id.to_string(), Arc::new(bundle))
            .map_err(|e| e.to_string())?;

        Ok(total)
    }

    pub fn deactivate(&self, id: &str, registries: &ResourceRegistries, tool_registry: &ToolRegistry) -> Result<Summary, String> {
        let plugin = self
            .resource_plugins
            .get(id)
            .ok_or_else(|| format!("ResourcePlugin \"{id}\" not found"))?;

        plugin.on_before_uninstall()?;

        let mut total = Summary::new();
        if let Some(bundle) = self.active_bundles.get(id) {
            for wf in &bundle.workflows {
                unregister_item(&registries.workflows, &wf.id, &mut total);
            }
            for tool in &bundle.tools {
                if tool_registry.remove_tool(&tool.id).is_some() {
                    total.merge(Summary::ok(&tool.id));
                }
            }
            for trigger in &bundle.triggers {
                unregister_item(&registries.trigger_templates, &trigger.name, &mut total);
            }
            for prompt in &bundle.prompts {
                unregister_item(&registries.templates, &prompt.id, &mut total);
            }
            for node_tmpl in &bundle.node_templates {
                unregister_item(&registries.node_templates, &node_tmpl.id, &mut total);
            }
            for agent_tmpl in &bundle.agent_templates {
                unregister_item(&registries.agent_templates, &agent_tmpl.id, &mut total);
            }
            self.active_bundles.unregister(id);
        }

        plugin.on_after_uninstall()?;

        Ok(total)
    }
}

impl Default for ResourcePluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn unregister_item<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: &str,
    total: &mut Summary,
) {
    if registry.unregister(key).is_some() {
        total.merge(Summary::ok(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predefined::agent_templates::builtin_agent_templates;

    struct TestResourcePlugin;

    impl ResourcePlugin for TestResourcePlugin {
        fn metadata(&self) -> ResourcePluginMetadata {
            ResourcePluginMetadata {
                id: "test-resource-plugin".into(),
                name: "Test ResourcePlugin".into(),
                version: "1.0.0".into(),
                description: "Test resource plugin".into(),
                author: None,
                tags: None,
                category: None,
                dependencies: None,
                configurable: None,
            }
        }

        fn assemble(&self, _config: &Value) -> Result<ResourceBundle, String> {
            let mut bundle = ResourceBundle::new();
            bundle.agent_templates = builtin_agent_templates();
            bundle.prompts.push(Template {
                id: "test.prompt".into(),
                name: "Test Prompt".into(),
                description: Some("Test".into()),
                category: "system".into(),
                content: "hello".into(),
                variables: None,
                fragments: None,
            });
            Ok(bundle)
        }
    }

    #[test]
    fn register_duplicate_rejected() {
        let registry = ResourcePluginRegistry::new();
        registry.register(Box::new(TestResourcePlugin)).unwrap();
        let err = registry.register(Box::new(TestResourcePlugin)).unwrap_err();
        assert!(err.contains("already"));
    }

    #[test]
    fn activate_unknown_resource_plugin_errors() {
        let registry = ResourcePluginRegistry::new();
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let err = registry
            .activate("missing", &Value::Null, &regs, &tool_registry, true)
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn activate_and_deactivate_roundtrip() {
        let registry = ResourcePluginRegistry::new();
        registry.register(Box::new(TestResourcePlugin)).unwrap();

        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let summary = registry
            .activate("test-resource-plugin", &Value::Null, &regs, &tool_registry, true)
            .unwrap();
        assert!(summary.is_ok());
        assert!(registry.is_active("test-resource-plugin"));

        // Every bundle item is registered.
        assert!(regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(regs.templates.has("test.prompt"));

        // Duplicate activation with skip_if_exists stays consistent.
        let again = registry
            .activate("test-resource-plugin", &Value::Null, &regs, &tool_registry, true)
            .unwrap();
        assert!(again.is_ok());

        // Deactivation removes every registered item and clears the tracking.
        let removed = registry.deactivate("test-resource-plugin", &regs, &tool_registry).unwrap();
        assert_eq!(removed.succeeded.len(), 3);
        assert!(!regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(!regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(!regs.templates.has("test.prompt"));
        assert!(!registry.is_active("test-resource-plugin"));

        // Deactivating twice is a no-op, not an error.
        let again = registry.deactivate("test-resource-plugin", &regs, &tool_registry).unwrap();
        assert!(again.succeeded.is_empty());
    }

    #[test]
    fn deactivate_unknown_resource_plugin_errors() {
        let registry = ResourcePluginRegistry::new();
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();
        let err = registry.deactivate("missing", &regs, &tool_registry).unwrap_err();
        assert!(err.contains("not found"));
    }
}
