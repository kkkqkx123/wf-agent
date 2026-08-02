use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{HookTemplate, NodeTemplate, WorkflowTemplate};
use wf_types::PromptTemplate;

use crate::registrar::{register_item, Registries};
use crate::result::Summary;

#[derive(Debug, Clone)]
pub struct Bundle {
    pub workflows: Vec<WorkflowTemplate>,
    pub tools: Vec<ToolDef>,
    pub triggers: Vec<TriggerTemplate>,
    pub prompts: Vec<PromptTemplate>,
    pub node_templates: Vec<NodeTemplate>,
    pub hook_templates: Vec<HookTemplate>,
    pub agent_templates: Vec<AgentTemplate>,
}

impl Bundle {
    pub fn new() -> Self {
        Self {
            workflows: Vec::new(),
            tools: Vec::new(),
            triggers: Vec::new(),
            prompts: Vec::new(),
            node_templates: Vec::new(),
            hook_templates: Vec::new(),
            agent_templates: Vec::new(),
        }
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StarterConfigFieldType {
    String,
    Number,
    Boolean,
    Expression,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StarterConfigField {
    pub r#type: StarterConfigFieldType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_functions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StarterMetadata {
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
    pub configurable: Option<HashMap<String, StarterConfigField>>,
}

pub trait Starter: Send + Sync {
    fn metadata(&self) -> StarterMetadata;
    fn assemble(&self, config: &Value) -> Result<Bundle, String>;

    fn on_before_assemble(&self, _config: &Value) -> Result<(), String> {
        Ok(())
    }
    fn on_after_install(&self, _bundle: &Bundle) -> Result<(), String> {
        Ok(())
    }
    fn on_before_uninstall(&self) -> Result<(), String> {
        Ok(())
    }
    fn on_after_uninstall(&self) -> Result<(), String> {
        Ok(())
    }
}

pub struct BundleRegistry {
    starters: ConcurrentRegistry<Box<dyn Starter>>,
    active_bundles: ConcurrentRegistry<Bundle>,
}

impl BundleRegistry {
    pub fn new() -> Self {
        Self {
            starters: ConcurrentRegistry::new(),
            active_bundles: ConcurrentRegistry::new(),
        }
    }

    pub fn register(&self, starter: Box<dyn Starter>) -> Result<(), String> {
        let id = starter.metadata().id;
        let item = Arc::new(starter);
        self.starters
            .register(id.clone(), item)
            .map_err(|e| e.to_string())
    }

    pub fn unregister(&self, id: &str) {
        self.starters.unregister(id);
    }

    pub fn get(&self, id: &str) -> Option<Arc<Box<dyn Starter>>> {
        self.starters.get(id)
    }

    pub fn list(&self) -> Vec<String> {
        self.starters.list()
    }

    pub fn is_active(&self, id: &str) -> bool {
        self.active_bundles.has(id)
    }

    pub fn activate(
        &self,
        id: &str,
        config: &Value,
        registries: &Registries,
        skip_if_exists: bool,
    ) -> Result<Summary, String> {
        let starter = self
            .starters
            .get(id)
            .ok_or_else(|| format!("Starter \"{id}\" not found"))?;

        starter.on_before_assemble(config)?;

        let bundle = starter.assemble(config)?;
        let mut total = Summary::new();

        for wf in &bundle.workflows {
            let key = wf.id.clone();
            total.merge(register_item(
                &registries.workflows,
                key,
                wf.clone(),
                skip_if_exists,
            ));
        }
        for tool in &bundle.tools {
            let key = tool.id.clone();
            total.merge(register_item(
                &registries.tools,
                key,
                tool.clone(),
                skip_if_exists,
            ));
        }
        for trigger in &bundle.triggers {
            let key = trigger.name.clone();
            total.merge(register_item(
                &registries.trigger_templates,
                key,
                trigger.clone(),
                skip_if_exists,
            ));
        }
        for prompt in &bundle.prompts {
            let key = prompt.id.clone();
            total.merge(register_item(
                &registries.prompt_templates,
                key,
                prompt.clone(),
                skip_if_exists,
            ));
        }
        for node_tmpl in &bundle.node_templates {
            let key = node_tmpl.id.clone();
            total.merge(register_item(
                &registries.node_templates,
                key,
                node_tmpl.clone(),
                skip_if_exists,
            ));
        }
        for hook_tmpl in &bundle.hook_templates {
            let key = hook_tmpl.id.clone();
            total.merge(register_item(
                &registries.hook_templates,
                key,
                hook_tmpl.clone(),
                skip_if_exists,
            ));
        }
        for agent_tmpl in &bundle.agent_templates {
            let key = agent_tmpl.id.clone();
            total.merge(register_item(
                &registries.agent_templates,
                key,
                agent_tmpl.clone(),
                skip_if_exists,
            ));
        }

        starter.on_after_install(&bundle)?;

        // Re-activation replaces the tracked bundle (mirrors the TS
        // activeBundles map semantics).
        self.active_bundles.unregister(id);
        self.active_bundles
            .register(id.to_string(), Arc::new(bundle))
            .map_err(|e| e.to_string())?;

        Ok(total)
    }

    pub fn deactivate(&self, id: &str, registries: &Registries) -> Result<Summary, String> {
        let starter = self
            .starters
            .get(id)
            .ok_or_else(|| format!("Starter \"{id}\" not found"))?;

        starter.on_before_uninstall()?;

        let mut total = Summary::new();
        if let Some(bundle) = self.active_bundles.get(id) {
            for wf in &bundle.workflows {
                unregister_item(&registries.workflows, &wf.id, &mut total);
            }
            for tool in &bundle.tools {
                unregister_item(&registries.tools, &tool.id, &mut total);
            }
            for trigger in &bundle.triggers {
                unregister_item(&registries.trigger_templates, &trigger.name, &mut total);
            }
            for prompt in &bundle.prompts {
                unregister_item(&registries.prompt_templates, &prompt.id, &mut total);
            }
            for node_tmpl in &bundle.node_templates {
                unregister_item(&registries.node_templates, &node_tmpl.id, &mut total);
            }
            for hook_tmpl in &bundle.hook_templates {
                unregister_item(&registries.hook_templates, &hook_tmpl.id, &mut total);
            }
            for agent_tmpl in &bundle.agent_templates {
                unregister_item(&registries.agent_templates, &agent_tmpl.id, &mut total);
            }
            self.active_bundles.unregister(id);
        }

        starter.on_after_uninstall()?;

        Ok(total)
    }
}

impl Default for BundleRegistry {
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

    struct TestStarter;

    impl Starter for TestStarter {
        fn metadata(&self) -> StarterMetadata {
            StarterMetadata {
                id: "test-starter".into(),
                name: "Test Starter".into(),
                version: "1.0.0".into(),
                description: "Test starter".into(),
                author: None,
                tags: None,
                category: None,
                dependencies: None,
                configurable: None,
            }
        }

        fn assemble(&self, _config: &Value) -> Result<Bundle, String> {
            let mut bundle = Bundle::new();
            bundle.agent_templates = builtin_agent_templates();
            bundle.prompts.push(PromptTemplate {
                id: "test.prompt".into(),
                name: "Test Prompt".into(),
                description: "Test".into(),
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
        let registry = BundleRegistry::new();
        registry.register(Box::new(TestStarter)).unwrap();
        let err = registry.register(Box::new(TestStarter)).unwrap_err();
        assert!(err.contains("already"));
    }

    #[test]
    fn activate_unknown_starter_errors() {
        let registry = BundleRegistry::new();
        let regs = Registries::new();
        let err = registry
            .activate("missing", &Value::Null, &regs, true)
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn activate_and_deactivate_roundtrip() {
        let registry = BundleRegistry::new();
        registry.register(Box::new(TestStarter)).unwrap();

        let regs = Registries::new();
        let summary = registry
            .activate("test-starter", &Value::Null, &regs, true)
            .unwrap();
        assert!(summary.is_ok());
        assert!(registry.is_active("test-starter"));

        // Every bundle item is registered.
        assert!(regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(regs.prompt_templates.has("test.prompt"));

        // Duplicate activation with skip_if_exists stays consistent.
        let again = registry
            .activate("test-starter", &Value::Null, &regs, true)
            .unwrap();
        assert!(again.is_ok());

        // Deactivation removes every registered item and clears the tracking.
        let removed = registry.deactivate("test-starter", &regs).unwrap();
        assert_eq!(removed.succeeded.len(), 3);
        assert!(!regs.agent_templates.has("@standard/goal-review-executor"));
        assert!(!regs.agent_templates.has("@standard/goal-review-reviewer"));
        assert!(!regs.prompt_templates.has("test.prompt"));
        assert!(!registry.is_active("test-starter"));

        // Deactivating twice is a no-op, not an error.
        let again = registry.deactivate("test-starter", &regs).unwrap();
        assert!(again.succeeded.is_empty());
    }

    #[test]
    fn deactivate_unknown_starter_errors() {
        let registry = BundleRegistry::new();
        let regs = Registries::new();
        let err = registry.deactivate("missing", &regs).unwrap_err();
        assert!(err.contains("not found"));
    }
}
