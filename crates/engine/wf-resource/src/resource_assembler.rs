//! Declarative resource bundles assembled from config.
//!
//! A `ResourceAssembler` here is a config-to-bundle assembler, not a
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
use wf_types::agent::{AgentDefinition, AgentTemplate};
use wf_types::node::configs::{LoopEndNodeConfig, LoopStartNodeConfig, LoopVariableInput};
use wf_types::node::{BaseStaticNode, StaticNodeType};
use wf_types::tool::AvailableTools;
use wf_types::tool::Tool as ToolDef;
use wf_types::tool_description::ToolDescriptionData;
use wf_types::trigger::TriggerTemplate;
use wf_types::workflow::{Edge, EdgeType, NodeTemplate, WorkflowTemplate};
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

    pub fn builder() -> ResourceBundleBuilder {
        ResourceBundleBuilder::new()
    }
}

/// Config parsing contract shared by all `ResourceAssembler` implementations.
///
/// Each assembler owns a config struct that parses itself from the raw JSON
/// value passed to `assemble()` and validates its own invariants. Keeping
/// parsing plus validation next to the config struct removes field-by-field
/// boilerplate from `assemble()` and gives every assembler the same shape.
pub trait AssemblerConfig: Sized {
    fn from_value(value: &Value) -> Result<Self, String>;
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }

    fn parse(value: &Value) -> Result<Self, String> {
        let config = Self::from_value(value)?;
        config.validate()?;
        Ok(config)
    }
}

/// Fluent composer for `ResourceBundle` values.
///
/// Every assembler builds the same eight resource lists; pushing to raw vecs
/// spreads that knowledge across each `assemble()`. The builder centralizes
/// it so `assemble()` reads as a declaration of what the bundle contains.
#[derive(Debug, Clone, Default)]
pub struct ResourceBundleBuilder {
    bundle: ResourceBundle,
}

impl ResourceBundleBuilder {
    pub fn new() -> Self {
        Self {
            bundle: ResourceBundle::new(),
        }
    }

    pub fn workflow(mut self, workflow: WorkflowTemplate) -> Self {
        self.bundle.workflows.push(workflow);
        self
    }

    pub fn workflows(mut self, workflows: impl IntoIterator<Item = WorkflowTemplate>) -> Self {
        self.bundle.workflows.extend(workflows);
        self
    }

    pub fn tool(mut self, tool: ToolDef) -> Self {
        self.bundle.tools.push(tool);
        self
    }

    pub fn tools(mut self, tools: impl IntoIterator<Item = ToolDef>) -> Self {
        self.bundle.tools.extend(tools);
        self
    }

    pub fn trigger(mut self, trigger: TriggerTemplate) -> Self {
        self.bundle.triggers.push(trigger);
        self
    }

    pub fn triggers(mut self, triggers: impl IntoIterator<Item = TriggerTemplate>) -> Self {
        self.bundle.triggers.extend(triggers);
        self
    }

    pub fn prompt(mut self, prompt: Template) -> Self {
        self.bundle.prompts.push(prompt);
        self
    }

    pub fn prompts(mut self, prompts: impl IntoIterator<Item = Template>) -> Self {
        self.bundle.prompts.extend(prompts);
        self
    }

    pub fn fragment(mut self, fragment: SystemPromptFragment) -> Self {
        self.bundle.fragments.push(fragment);
        self
    }

    pub fn fragments(mut self, fragments: impl IntoIterator<Item = SystemPromptFragment>) -> Self {
        self.bundle.fragments.extend(fragments);
        self
    }

    pub fn tool_description(mut self, description: ToolDescriptionData) -> Self {
        self.bundle.tool_descriptions.push(description);
        self
    }

    pub fn tool_descriptions(
        mut self,
        descriptions: impl IntoIterator<Item = ToolDescriptionData>,
    ) -> Self {
        self.bundle.tool_descriptions.extend(descriptions);
        self
    }

    pub fn node_template(mut self, template: NodeTemplate) -> Self {
        self.bundle.node_templates.push(template);
        self
    }

    pub fn node_templates(mut self, templates: impl IntoIterator<Item = NodeTemplate>) -> Self {
        self.bundle.node_templates.extend(templates);
        self
    }

    pub fn agent_template(mut self, template: AgentTemplate) -> Self {
        self.bundle.agent_templates.push(template);
        self
    }

    pub fn agent_templates(mut self, templates: impl IntoIterator<Item = AgentTemplate>) -> Self {
        self.bundle.agent_templates.extend(templates);
        self
    }

    pub fn build(self) -> ResourceBundle {
        self.bundle
    }
}

impl From<ResourceBundleBuilder> for ResourceBundle {
    fn from(builder: ResourceBundleBuilder) -> Self {
        builder.build()
    }
}

/// Merge config overrides into a base agent template and return the inline
/// `AgentDefinition` the AGENT_LOOP handler requires.
///
/// `None` leaves the corresponding base field untouched; providing a tool
/// list replaces the base list wholesale.
pub fn merge_agent_config(
    template: &AgentTemplate,
    profile_id: Option<String>,
    system_prompt: Option<String>,
    max_iterations: Option<u32>,
    tools: Option<Vec<String>>,
) -> AgentDefinition {
    let mut definition = template.definition.clone();
    if let Some(config) = definition.config.as_mut() {
        if let Some(id) = profile_id {
            config.profile_id = Some(id);
        }
        if let Some(prompt) = system_prompt {
            config.system_prompt = Some(prompt);
        }
        if let Some(iterations) = max_iterations {
            config.max_iterations = Some(iterations);
        }
        if let Some(tools) = tools {
            let available = config
                .available_tools
                .get_or_insert_with(|| AvailableTools {
                    available: Vec::new(),
                    initial: None,
                    discoverable: None,
                    enable_general_tool: None,
                    hidden: None,
                    require_approval: None,
                    allowed_workflows: None,
                });
            available.available = tools;
        }
    }
    definition
}

/// Fluent customization of a base `AgentTemplate` into an inline definition.
///
/// Every assembler that embeds agents overrides the same four config knobs;
/// the builder centralizes that merge so `assemble()` declares overrides
/// instead of cloning configs by hand.
#[derive(Debug, Clone)]
pub struct AgentTemplateBuilder {
    base: AgentTemplate,
    profile_id: Option<String>,
    system_prompt: Option<String>,
    max_iterations: Option<u32>,
    tools: Option<Vec<String>>,
}

impl AgentTemplateBuilder {
    pub fn new(base: AgentTemplate) -> Self {
        Self {
            base,
            profile_id: None,
            system_prompt: None,
            max_iterations: None,
            tools: None,
        }
    }

    pub fn with_profile(mut self, id: impl Into<String>) -> Self {
        self.profile_id = Some(id.into());
        self
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = Some(n);
        self
    }

    pub fn with_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tools = Some(tools.into_iter().map(Into::into).collect());
        self
    }

    pub fn maybe_profile(mut self, id: Option<String>) -> Self {
        if let Some(id) = id {
            self.profile_id = Some(id);
        }
        self
    }

    pub fn maybe_system_prompt(mut self, prompt: Option<String>) -> Self {
        if let Some(prompt) = prompt {
            self.system_prompt = Some(prompt);
        }
        self
    }

    pub fn maybe_max_iterations(mut self, n: Option<u32>) -> Self {
        if let Some(n) = n {
            self.max_iterations = Some(n);
        }
        self
    }

    pub fn maybe_tools(mut self, tools: Option<Vec<String>>) -> Self {
        if let Some(tools) = tools {
            self.tools = Some(tools);
        }
        self
    }

    pub fn build_inline(self) -> AgentDefinition {
        merge_agent_config(
            &self.base,
            self.profile_id,
            self.system_prompt,
            self.max_iterations,
            self.tools,
        )
    }
}

/// Build a static workflow edge.
pub fn workflow_edge(
    id: &str,
    source: &str,
    target: &str,
    r#type: EdgeType,
    condition: Option<String>,
) -> Edge {
    Edge {
        id: id.into(),
        source_node_id: source.into(),
        target_node_id: target.into(),
        r#type,
        condition,
        label: None,
        description: None,
        weight: None,
        metadata: None,
        error_route: None,
    }
}

/// Fluent wiring for a LOOP_START / LOOP_END review-style loop segment.
///
/// The engine pairs loops by `loop_id` and follows `loop_start_node_id`
/// back on continuation, so the builder owns both boundary nodes plus the
/// loop-back edge and derives them from one `loop_id`. Callers supply the
/// body nodes with the edges between them; entry edges into LOOP_START and
/// exit edges out of LOOP_END stay with the caller because they belong to
/// the surrounding graph, not the loop itself.
#[derive(Debug, Clone, Default)]
pub struct LoopWorkflowBuilder {
    loop_id: String,
    loop_start_id: String,
    loop_end_id: String,
    loop_start_name: Option<String>,
    loop_end_name: Option<String>,
    max_iterations: Option<u32>,
    variable_inputs: Vec<LoopVariableInput>,
    break_condition: Option<String>,
    continue_condition: Option<String>,
    loop_back_edge_id: Option<String>,
    body_nodes: Vec<BaseStaticNode>,
    body_edges: Vec<Edge>,
}

impl LoopWorkflowBuilder {
    pub fn new(loop_id: &str, loop_start_id: &str, loop_end_id: &str) -> Self {
        Self {
            loop_id: loop_id.to_string(),
            loop_start_id: loop_start_id.to_string(),
            loop_end_id: loop_end_id.to_string(),
            ..Self::default()
        }
    }

    pub fn loop_start_name(mut self, name: impl Into<String>) -> Self {
        self.loop_start_name = Some(name.into());
        self
    }

    pub fn loop_end_name(mut self, name: impl Into<String>) -> Self {
        self.loop_end_name = Some(name.into());
        self
    }

    pub fn max_iterations(mut self, n: u32) -> Self {
        self.max_iterations = Some(n);
        self
    }

    pub fn variable_inputs(mut self, inputs: Vec<LoopVariableInput>) -> Self {
        self.variable_inputs = inputs;
        self
    }

    pub fn variable_input(mut self, input: LoopVariableInput) -> Self {
        self.variable_inputs.push(input);
        self
    }

    pub fn break_condition(mut self, condition: impl Into<String>) -> Self {
        self.break_condition = Some(condition.into());
        self
    }

    pub fn continue_condition(mut self, condition: impl Into<String>) -> Self {
        self.continue_condition = Some(condition.into());
        self
    }

    pub fn loop_back_edge_id(mut self, id: impl Into<String>) -> Self {
        self.loop_back_edge_id = Some(id.into());
        self
    }

    pub fn body_node(mut self, node: BaseStaticNode) -> Self {
        self.body_nodes.push(node);
        self
    }

    pub fn body_nodes(mut self, nodes: impl IntoIterator<Item = BaseStaticNode>) -> Self {
        self.body_nodes.extend(nodes);
        self
    }

    pub fn body_edge(mut self, edge: Edge) -> Self {
        self.body_edges.push(edge);
        self
    }

    pub fn body_edges(mut self, edges: impl IntoIterator<Item = Edge>) -> Self {
        self.body_edges.extend(edges);
        self
    }

    pub fn build(self) -> Result<(Vec<BaseStaticNode>, Vec<Edge>), String> {
        let max_iterations = self.max_iterations.filter(|n| *n > 0).ok_or_else(|| {
            format!(
                "LoopWorkflowBuilder '{}': max_iterations must be set to a value greater than 0",
                self.loop_id
            )
        })?;

        let start_config = LoopStartNodeConfig {
            loop_id: self.loop_id.clone(),
            variable_inputs: Some(self.variable_inputs.clone()),
            data_source: None,
            max_iterations,
            break_condition: None,
        };
        let start_config = serde_json::to_value(&start_config)
            .map_err(|e| format!("LoopWorkflowBuilder '{}': {e}", self.loop_id))?;
        let end_config = LoopEndNodeConfig {
            loop_id: self.loop_id.clone(),
            break_condition: self.break_condition.clone(),
            loop_start_node_id: Some(self.loop_start_id.clone()),
        };
        let end_config = serde_json::to_value(&end_config)
            .map_err(|e| format!("LoopWorkflowBuilder '{}': {e}", self.loop_id))?;

        let mut nodes = Vec::with_capacity(self.body_nodes.len() + 2);
        nodes.push(BaseStaticNode {
            id: self.loop_start_id.clone(),
            node_type: StaticNodeType::LoopStart,
            name: self.loop_start_name.clone(),
            description: None,
            config: Some(start_config),
            execution_config: None,
        });
        nodes.extend(self.body_nodes);
        nodes.push(BaseStaticNode {
            id: self.loop_end_id.clone(),
            node_type: StaticNodeType::LoopEnd,
            name: self.loop_end_name.clone(),
            description: None,
            config: Some(end_config),
            execution_config: None,
        });

        // The Rust engine treats LOOP_END -> LOOP_START as the legal loop
        // continuation (the LOOP_END handler jumps back via
        // loop_start_node_id).
        let mut edges = self.body_edges;
        edges.push(workflow_edge(
            &self
                .loop_back_edge_id
                .unwrap_or_else(|| format!("{}_to_{}", self.loop_end_id, self.loop_start_id)),
            &self.loop_end_id,
            &self.loop_start_id,
            EdgeType::Conditional,
            self.continue_condition.clone(),
        ));
        Ok((nodes, edges))
    }
}

impl Default for ResourceBundle {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceAssemblerConfigFieldType {
    String,
    Number,
    Boolean,
    Expression,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceAssemblerConfigField {
    pub r#type: ResourceAssemblerConfigFieldType,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_functions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceAssemblerMetadata {
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
    pub configurable: Option<HashMap<String, ResourceAssemblerConfigField>>,
}

pub trait ResourceAssembler: Send + Sync {
    fn metadata(&self) -> ResourceAssemblerMetadata;
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
        let mut prompt = prompt.clone();
        // A prompt template registered under the same id overrides the
        // bundle's prompt at the install edge, mirroring the agent-template
        // override; placeholder-carrying overrides are rejected because
        // install-time patching cannot render them.
        match crate::predefined::agent_prompts::bundle_prompt_override(registries, &prompt) {
            crate::predefined::agent_prompts::BundlePromptOverride::Patch(content) => {
                prompt.content = content;
            }
            crate::predefined::agent_prompts::BundlePromptOverride::Reject(message) => {
                total.merge(Summary::err(&prompt.id, message));
                continue;
            }
            crate::predefined::agent_prompts::BundlePromptOverride::KeepBundle => {}
        }
        total.merge(register_template(registries, prompt, skip_if_exists));
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
        // A prompt template registered under the same `@standard/*` id
        // overrides the embedded system prompt, matching the
        // predefined agent-template registration edge.
        let mut agent_tmpl = agent_tmpl.clone();
        if let Some(prompt) =
            crate::predefined::agent_prompts::resolve_system_prompt(registries, &agent_tmpl.id)
        {
            if let Some(config) = agent_tmpl.definition.config.as_mut() {
                config.system_prompt = Some(prompt);
            }
        }
        total.merge(if skip_if_exists {
            register_item_skip(&registries.agent_templates, key, agent_tmpl)
        } else {
            register_item_strict(&registries.agent_templates, key, agent_tmpl)
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
            crate::predefined::resource_assembler::goal_review::agent::goal_review_executor(),
            crate::predefined::resource_assembler::goal_review::agent::goal_review_reviewer(),
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
    fn install_applies_prompt_override_to_agent_templates() {
        let regs = ResourceRegistries::new();
        let tool_registry = ToolRegistry::new();

        // A prompt template registered under the executor's `@standard/*` id
        // overrides the bundle agent template's embedded system prompt.
        crate::registry::register_item_skip(
            &regs.templates,
            "@standard/goal-review-executor".into(),
            Template {
                id: "@standard/goal-review-executor".into(),
                name: "Override".into(),
                description: None,
                category: "system".into(),
                content: "custom executor prompt".into(),
                variables: None,
                fragments: None,
            },
        );

        let summary = install_bundle(&regs, &tool_registry, &bundle(), true);
        assert!(summary.is_ok());

        let executor = regs
            .agent_templates
            .get("@standard/goal-review-executor")
            .expect("executor installed");
        let prompt = executor
            .definition
            .config
            .as_ref()
            .and_then(|c| c.system_prompt.as_deref())
            .expect("system prompt present");
        assert_eq!(prompt, "custom executor prompt");

        // The reviewer has no override and keeps its embedded prompt.
        let reviewer = regs
            .agent_templates
            .get("@standard/goal-review-reviewer")
            .expect("reviewer installed");
        let reviewer_prompt = reviewer
            .definition
            .config
            .as_ref()
            .and_then(|c| c.system_prompt.as_deref())
            .expect("system prompt present");
        assert_ne!(reviewer_prompt, "custom executor prompt");
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

    #[test]
    fn builder_composes_bundle_fluently() {
        let bundle = ResourceBundle::builder()
            .prompt(Template {
                id: "test.prompt".into(),
                name: "Test Prompt".into(),
                description: None,
                category: "system".into(),
                content: "hello".into(),
                variables: None,
                fragments: None,
            })
            .agent_templates(vec![
                crate::predefined::resource_assembler::goal_review::agent::goal_review_executor(),
                crate::predefined::resource_assembler::goal_review::agent::goal_review_reviewer(),
            ])
            .build();

        assert_eq!(bundle.prompts.len(), 1);
        assert_eq!(bundle.agent_templates.len(), 2);
        assert!(bundle.workflows.is_empty());
    }

    #[test]
    fn assembler_config_parse_runs_validation() {
        struct StrictConfig {
            name: String,
        }

        impl AssemblerConfig for StrictConfig {
            fn from_value(value: &Value) -> Result<Self, String> {
                let name = value
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| "config requires 'name' (string)".to_string())?
                    .to_string();
                Ok(Self { name })
            }

            fn validate(&self) -> Result<(), String> {
                if self.name.is_empty() {
                    return Err("'name' must not be empty".to_string());
                }
                Ok(())
            }
        }

        assert!(StrictConfig::parse(&serde_json::json!({"name": "ok"})).is_ok());
        assert!(StrictConfig::parse(&serde_json::json!({})).is_err());
        assert!(StrictConfig::parse(&serde_json::json!({"name": ""})).is_err());
    }

    #[test]
    fn agent_builder_applies_overrides() {
        let base =
            crate::predefined::resource_assembler::goal_review::agent::goal_review_executor();
        let definition = AgentTemplateBuilder::new(base)
            .with_profile("custom-exec")
            .with_system_prompt("be terse")
            .with_max_iterations(42)
            .with_tools(["read_file", "grep"])
            .build_inline();

        let config = definition.config.expect("agent config");
        assert_eq!(config.profile_id.as_deref(), Some("custom-exec"));
        assert_eq!(config.system_prompt.as_deref(), Some("be terse"));
        assert_eq!(config.max_iterations, Some(42));
        assert_eq!(
            config.available_tools.expect("tools").available,
            vec!["read_file", "grep"]
        );
    }

    #[test]
    fn agent_builder_maybe_helpers_skip_none() {
        let base =
            crate::predefined::resource_assembler::goal_review::agent::goal_review_reviewer();
        let untouched = AgentTemplateBuilder::new(base.clone())
            .maybe_profile(None)
            .maybe_system_prompt(None)
            .maybe_max_iterations(None)
            .maybe_tools(None)
            .build_inline();
        assert_eq!(untouched, base.definition);

        let touched = AgentTemplateBuilder::new(base)
            .maybe_system_prompt(Some("be strict".to_string()))
            .build_inline();
        assert_eq!(
            touched
                .config
                .expect("agent config")
                .system_prompt
                .as_deref(),
            Some("be strict")
        );
    }

    #[test]
    fn loop_builder_wires_loop_back_edge() {
        let body = BaseStaticNode {
            id: "work".into(),
            node_type: StaticNodeType::Llm,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        };
        let (nodes, edges) = LoopWorkflowBuilder::new("demo-loop", "loop_start", "loop_end")
            .loop_start_name("Start")
            .loop_end_name("End")
            .max_iterations(3)
            .variable_input(LoopVariableInput {
                source_path: "status".into(),
                internal_name: "status".into(),
                required: Some(true),
                default_value: None,
                description: None,
            })
            .break_condition("eq(status,\"done\")")
            .continue_condition("eq(nextIteration,true)")
            .loop_back_edge_id("e-back")
            .body_node(body)
            .body_edge(workflow_edge(
                "e0",
                "loop_start",
                "work",
                EdgeType::Default,
                None,
            ))
            .body_edge(workflow_edge(
                "e1",
                "work",
                "loop_end",
                EdgeType::Default,
                None,
            ))
            .build()
            .expect("loop segment");

        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].node_type, StaticNodeType::LoopStart);
        assert_eq!(nodes[2].node_type, StaticNodeType::LoopEnd);

        let start_config = nodes[0].config.as_ref().expect("start config");
        assert_eq!(start_config["loop_id"].as_str(), Some("demo-loop"));
        assert_eq!(start_config["max_iterations"].as_u64(), Some(3));

        let end_config = nodes[2].config.as_ref().expect("end config");
        assert_eq!(end_config["loop_id"].as_str(), Some("demo-loop"));
        assert_eq!(
            end_config["break_condition"].as_str(),
            Some("eq(status,\"done\")")
        );
        assert_eq!(
            end_config["loop_start_node_id"].as_str(),
            Some("loop_start")
        );

        assert_eq!(edges.len(), 3);
        let back = edges.iter().find(|e| e.id == "e-back").expect("loop-back");
        assert_eq!(back.source_node_id, "loop_end");
        assert_eq!(back.target_node_id, "loop_start");
        assert_eq!(back.r#type, EdgeType::Conditional);
        assert_eq!(back.condition.as_deref(), Some("eq(nextIteration,true)"));
    }

    #[test]
    fn loop_builder_requires_max_iterations() {
        let missing = LoopWorkflowBuilder::new("demo-loop", "loop_start", "loop_end").build();
        assert!(missing.is_err());

        let zero = LoopWorkflowBuilder::new("demo-loop", "loop_start", "loop_end")
            .max_iterations(0)
            .build();
        assert!(zero.is_err());
    }
}
