use std::sync::Arc;

use std::path::Path;

use wf_core::registry::{ConcurrentRegistry, MutableRegistry, Registry};
use wf_types::agent::AgentTemplate;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{HookTemplate, NodeTemplate, WorkflowTemplate};
use wf_types::{PromptTemplate, SystemPromptFragment};

use crate::custom;
use crate::predefined;
use crate::result::Summary;
use crate::starter::BundleRegistry;

pub static PREDEFINED_FRAGMENT_IDS: &[&str] = &[
    "fragments.role.assistant",
    "fragments.role.coder",
    "fragments.role.analyst",
    "fragments.capability.general",
    "fragments.capability.general-principles",
    "fragments.capability.coding",
    "fragments.constraint.general",
    "fragments.constraint.general-interaction",
    "fragments.constraint.coding",
    "fragments.constraint.code-safety",
    "fragments.tool-usage.xml-summary",
    "fragments.tool-usage.json-summary",
    "fragments.task-instruction.code-review",
    "fragments.task-instruction.data-analysis",
];

pub static PREDEFINED_PROMPT_IDS: &[&str] = &[
    "system.default",
    "system.code",
    "system.agent",
];

pub static PREDEFINED_TOOL_DESCRIPTION_IDS: &[&str] = &[
    "read_file",
    "write_file",
    "edit_file",
    "list_dir",
    "grep_search",
    "glob_search",
    "execute_command",
    "web_search",
    "web_fetch",
    "memory_remember",
    "memory_forget",
    "memory_list",
    "apply_patch",
    "apply_diff",
    "skill",
    "use_mcp",
    "record_note",
    "recall_notes",
    "backend_shell",
    "shell_output",
    "shell_kill",
    "ask_followup_question",
    "attempt_completion",
    "execute_workflow",
    "query_workflow_status",
    "cancel_workflow",
    "call_agent",
];

#[derive(Debug, Clone)]
pub struct StarterActivation {
    pub id: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub skip_if_exists: bool,
    pub allow_list: Option<Vec<String>>,
    pub block_list: Option<Vec<String>>,
    pub custom_resources: Option<crate::custom::types::CustomResourcesPresetConfig>,
    pub custom_base_dir: Option<String>,
    pub starter_activation: Option<StarterActivation>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            skip_if_exists: true,
            allow_list: None,
            block_list: None,
            custom_resources: None,
            custom_base_dir: None,
            starter_activation: None,
        }
    }
}

pub struct Registries {
    pub fragments: ConcurrentRegistry<SystemPromptFragment>,
    pub prompt_templates: ConcurrentRegistry<PromptTemplate>,
    pub tool_descriptions: ConcurrentRegistry<ToolDescriptionData>,
    pub tools: ConcurrentRegistry<ToolDef>,
    pub trigger_templates: ConcurrentRegistry<TriggerTemplate>,
    pub workflows: ConcurrentRegistry<WorkflowTemplate>,
    pub node_templates: ConcurrentRegistry<NodeTemplate>,
    pub hook_templates: ConcurrentRegistry<HookTemplate>,
    pub agent_templates: ConcurrentRegistry<AgentTemplate>,
}

impl Registries {
    pub fn new() -> Self {
        Self {
            fragments: ConcurrentRegistry::new(),
            prompt_templates: ConcurrentRegistry::new(),
            tool_descriptions: ConcurrentRegistry::new(),
            tools: ConcurrentRegistry::new(),
            trigger_templates: ConcurrentRegistry::new(),
            workflows: ConcurrentRegistry::new(),
            node_templates: ConcurrentRegistry::new(),
            hook_templates: ConcurrentRegistry::new(),
            agent_templates: ConcurrentRegistry::new(),
        }
    }
}

impl Default for Registries {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register_item<T: Send + Sync>(
    registry: &ConcurrentRegistry<T>,
    key: String,
    item: T,
    skip_if_exists: bool,
) -> Summary {
    if skip_if_exists && registry.has(&key) {
        return Summary::ok(&key);
    }

    let item = Arc::new(item);
    match registry.register(key.clone(), item) {
        Ok(()) => Summary::ok(key),
        Err(e) => Summary::err(key, e.to_string()),
    }
}

pub fn register_all(regs: &Registries, bundle_reg: &BundleRegistry, opts: &Options) -> Summary {
    let mut total = Summary::new();

    // Pipeline 1: Predefined resources
    total.merge(predefined::fragments::register(regs, opts));
    total.merge(predefined::prompts::register(regs, opts));
    total.merge(predefined::tool_descriptions::register(regs, opts));
    total.merge(predefined::agent_templates::register(regs, opts));
    total.merge(predefined::tools::register(regs, opts));
    total.merge(predefined::triggers::register(regs, opts));
    total.merge(predefined::workflows::register(regs, opts));

    // Pipeline 2: Custom resources (from config)
    if let Some(ref custom_config) = opts.custom_resources {
        let base_dir_str = opts.custom_base_dir.as_deref().unwrap_or(".");
        let base_dir = Path::new(base_dir_str);
        let resources = custom::loader::load_custom_resources(custom_config, base_dir);
        total.merge(custom::register::register_custom_resources(regs, resources, opts.skip_if_exists));
    }

    // Pipeline 3: Starter activation
    if let Some(ref sa) = opts.starter_activation {
        match bundle_reg.activate(&sa.id, &sa.config, regs, opts.skip_if_exists) {
            Ok(s) => total.merge(s),
            Err(e) => total.merge(Summary::err(&sa.id, e)),
        }
    }

    total
}

// ── helpers ──────────────────────────────────────────────

pub fn is_resource_disabled(id: &str, opts: &Options) -> bool {
    if let Some(ref allow) = opts.allow_list {
        return !allow.contains(&id.to_string());
    }
    if let Some(ref block) = opts.block_list {
        return block.contains(&id.to_string());
    }
    false
}

pub fn are_fragments_registered(regs: &Registries) -> bool {
    PREDEFINED_FRAGMENT_IDS
        .iter()
        .all(|id| regs.fragments.has(id))
}

pub fn are_prompt_templates_registered(regs: &Registries) -> bool {
    PREDEFINED_PROMPT_IDS
        .iter()
        .all(|id| regs.prompt_templates.has(id))
}

pub fn are_predefined_tool_descriptions_registered(regs: &Registries) -> bool {
    PREDEFINED_TOOL_DESCRIPTION_IDS
        .iter()
        .all(|id| regs.tool_descriptions.has(id))
}

pub fn unregister_predefined_content(regs: &Registries) -> Summary {
    let mut total = Summary::new();

    for id in PREDEFINED_FRAGMENT_IDS {
        if regs.fragments.unregister(id).is_some() {
            total.merge(Summary::ok(*id));
        }
    }
    for id in PREDEFINED_PROMPT_IDS {
        if regs.prompt_templates.unregister(id).is_some() {
            total.merge(Summary::ok(*id));
        }
    }
    for id in PREDEFINED_TOOL_DESCRIPTION_IDS {
        if regs.tool_descriptions.unregister(id).is_some() {
            total.merge(Summary::ok(*id));
        }
    }

    total
}
