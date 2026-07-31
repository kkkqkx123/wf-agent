use std::sync::Arc;

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

pub trait Starter: Send + Sync {
    fn id(&self) -> &str;
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
}

impl BundleRegistry {
    pub fn new() -> Self {
        Self {
            starters: ConcurrentRegistry::new(),
        }
    }

    pub fn register(&self, starter: Box<dyn Starter>) -> Result<(), String> {
        let id = starter.id().to_string();
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

        Ok(total)
    }

    pub fn deactivate(&self, id: &str, registries: &Registries) -> Result<Summary, String> {
        let starter = self
            .starters
            .get(id)
            .ok_or_else(|| format!("Starter \"{id}\" not found"))?;

        starter.on_before_uninstall()?;

        let mut total = Summary::new();
        if registries.workflows.get(id).is_some() {
            let wid = format!("{}-workflow", id);
            if registries.workflows.unregister(&wid).is_some() {
                total.merge(Summary::ok(wid));
            }
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
