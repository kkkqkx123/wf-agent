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
    system_prompt_template_id: Option<String>,
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
        if let Some(id) = system_prompt_template_id {
            config.system_prompt_template_id = Some(id);
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
    system_prompt_template_id: Option<String>,
    max_iterations: Option<u32>,
    tools: Option<Vec<String>>,
}

impl AgentTemplateBuilder {
    pub fn new(base: AgentTemplate) -> Self {
        Self {
            base,
            profile_id: None,
            system_prompt: None,
            system_prompt_template_id: None,
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

    pub fn with_system_prompt_template_id(mut self, id: impl Into<String>) -> Self {
        self.system_prompt_template_id = Some(id.into());
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

    pub fn maybe_system_prompt_template_id(mut self, id: Option<String>) -> Self {
        if let Some(id) = id {
            self.system_prompt_template_id = Some(id);
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
            self.system_prompt_template_id,
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

impl ResourceAssemblerConfigFieldType {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Expression => "expression",
            Self::Array => "array",
            Self::Object => "object",
        }
    }

    pub fn matches_value(&self, value: &Value) -> bool {
        match self {
            Self::String => value.is_string(),
            Self::Number => value.is_number(),
            Self::Boolean => value.is_boolean(),
            Self::Expression => value.is_string(),
            Self::Array => value.is_array(),
            Self::Object => value.is_object(),
        }
    }
}

/// Declared-but-unset contract for assembler configs.
///
/// Two layers share the work: the metadata gate below reports missing
/// required keys and type mismatches with field names, while each
/// assembler's `AssemblerConfig::from_value` parses with `serde` and
/// `deny_unknown_fields` so misspelled keys fail fast with a path.
/// The strict entry [`gate_assembler_config`] additionally rejects
/// unknown keys; the lenient [`unknown_assembler_config_keys`] helper
/// stays for third-party assemblers that prefer warn-and-ignore forward
/// compatibility.
pub fn unknown_assembler_config_keys(
    value: &Value,
    metadata: &ResourceAssemblerMetadata,
) -> Vec<String> {
    let Some(configurable) = metadata.configurable.as_ref() else {
        return Vec::new();
    };
    let Value::Object(map) = value else {
        return Vec::new();
    };
    let mut unknown: Vec<String> = map
        .keys()
        .filter(|key| !configurable.contains_key(*key))
        .cloned()
        .collect();
    unknown.sort();
    unknown
}

pub fn validate_config_value_against_metadata(
    value: &Value,
    metadata: &ResourceAssemblerMetadata,
) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(configurable) = metadata.configurable.as_ref() else {
        return errors;
    };
    let object = match value {
        Value::Null => return errors,
        Value::Object(map) => map,
        _ => {
            errors.push("assembler config must be a JSON object".to_string());
            return errors;
        }
    };
    for (name, field) in configurable {
        match object.get(name) {
            None => {
                if field.required == Some(true) && field.default.is_none() {
                    errors.push(format!("assembler config missing required '{name}'"));
                }
            }
            Some(provided) if provided.is_null() => {
                if field.required == Some(true) && field.default.is_none() {
                    errors.push(format!("assembler config missing required '{name}'"));
                }
            }
            Some(provided) => {
                if !field.r#type.matches_value(provided) {
                    errors.push(format!(
                        "assembler config '{name}' expects {}, got {}",
                        field.r#type.type_name(),
                        config_value_kind(provided),
                    ));
                } else {
                    if matches!(
                        field.r#type,
                        ResourceAssemblerConfigFieldType::Expression
                    ) {
                        errors.extend(expression_allowlist_violations(name, provided, field));
                    }
                    errors.extend(assembler_nested_violations(name, provided, field));
                }
            }
        }
        if let Some(default) = field.default.as_ref() {
            if !field.r#type.matches_value(default) {
                errors.push(format!(
                    "assembler metadata default for '{name}' expects {}, got {}",
                    field.r#type.type_name(),
                    config_value_kind(default),
                ));
            }
        }
    }
    errors
}

/// Strict pre-assemble gate shared by built-in assemblers and the
/// runtime bridge. Merges metadata type and required errors with
/// unknown-key rejections; an empty object stands in for `Null` so a
/// missing config reports its missing required keys instead of a late
/// parse error.
pub fn gate_assembler_config(
    config: &Value,
    metadata: &ResourceAssemblerMetadata,
) -> Result<(), String> {
    let effective: Value = match config {
        Value::Null => Value::Object(Default::default()),
        other => other.clone(),
    };
    let mut errors = validate_config_value_against_metadata(&effective, metadata);
    let mut unknown = unknown_assembler_config_keys(&effective, metadata);
    unknown.sort();
    for key in unknown {
        errors.push(format!("assembler config has unknown key '{key}'"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Fill absent assembler keys from declared defaults. Explicit null
/// counts as missing: a configured default backfills it, otherwise the
/// key is dropped so the required check reports it once.
pub fn apply_assembler_config_defaults(
    config: &Value,
    metadata: &ResourceAssemblerMetadata,
) -> Value {
    let Some(configurable) = metadata.configurable.as_ref() else {
        return config.clone();
    };
    let mut object = match config {
        Value::Null => serde_json::Map::new(),
        Value::Object(map) => map.clone(),
        other => return other.clone(),
    };
    for (name, field) in configurable {
        let missing = object.get(name).is_none_or(|v| v.is_null());
        if !missing {
            continue;
        }
        if let Some(default) = field.default.as_ref() {
            if !default.is_null() {
                object.insert(name.clone(), default.clone());
                continue;
            }
        }
        object.remove(name);
    }
    Value::Object(object)
}

/// Function names called in an expression value: identifiers directly
/// followed by an opening parenthesis. String literals are skipped so
/// text like `"call me (later)"` is never mistaken for a call.
fn expression_called_functions(value: &str) -> Vec<String> {
    let bytes = value.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    let mut quote: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        let starts_name = byte.is_ascii_alphabetic() || byte == b'_';
        if !starts_name {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        let mut cursor = index;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor < bytes.len() && bytes[cursor] == b'(' {
            let name = value[start..index].to_string();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// Allowlist violations for an expression-typed config value. No
/// allowlist means no restriction; otherwise every called function name
/// must appear in it. All violations are returned together.
fn expression_allowlist_violations(
    name: &str,
    provided: &Value,
    field: &ResourceAssemblerConfigField,
) -> Vec<String> {
    let Some(allowed) = field.allowed_functions.as_ref() else {
        return Vec::new();
    };
    let Value::String(text) = provided else {
        return Vec::new();
    };
    expression_called_functions(text)
        .into_iter()
        .filter(|called| !allowed.contains(called))
        .map(|called| {
            format!(
                "assembler config '{name}' calls unknown function '{called}' (allowed: {})",
                allowed.join(", "),
            )
        })
        .collect()
}

/// One level of nesting for an already kind-matched assembler value:
/// object properties and array element types.
fn assembler_nested_violations(
    name: &str,
    provided: &Value,
    field: &ResourceAssemblerConfigField,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Some(properties) = field.properties.as_ref() {
        if let Value::Object(map) = provided {
            for (prop_name, prop_rule) in properties {
                match map.get(prop_name) {
                    None | Some(Value::Null) => {
                        if prop_rule.required == Some(true) {
                            errors.push(format!(
                                "assembler config '{name}.{prop_name}' is missing required property"
                            ));
                        }
                    }
                    Some(prop_value) => {
                        if !prop_rule.r#type.matches_value(prop_value) {
                            errors.push(format!(
                                "assembler config '{name}.{prop_name}' expects {}, got {}",
                                prop_rule.r#type.type_name(),
                                config_value_kind(prop_value),
                            ));
                        }
                    }
                }
            }
        }
    }
    if let Some(element_type) = field.element_type.as_ref() {
        if let Value::Array(items) = provided {
            for (index, item) in items.iter().enumerate() {
                if item.is_null() {
                    continue;
                }
                if !element_type.matches_value(item) {
                    errors.push(format!(
                        "assembler config '{name}[{index}]' expects {}, got {}",
                        element_type.type_name(),
                        config_value_kind(item),
                    ));
                }
            }
        }
    }
    errors
}

fn config_value_kind(value: &Value) -> &'static str {
    if value.is_null() {
        "null"
    } else if value.is_array() {
        "array"
    } else if value.is_string() {
        "string"
    } else if value.is_number() {
        "number"
    } else if value.is_boolean() {
        "boolean"
    } else {
        "object"
    }
}

/// Nested property rule for an object-typed assembler field. One level
/// only; deeper nesting is checked as the top-level kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceAssemblerNestedField {
    pub r#type: ResourceAssemblerConfigFieldType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
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
    /// One level of object property rules. `None` keeps the top-level
    /// kind check only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, ResourceAssemblerNestedField>>,
    /// Expected element kind for array values. Explicit null elements
    /// count as missing and skip the check.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_type: Option<ResourceAssemblerConfigFieldType>,
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
        total.merge(register_template(
            registries,
            prompt.clone(),
            skip_if_exists,
        ));
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
        // is what the installed agent template resolves its system prompt
        // through; the agent carries the reference, not a copied prompt.
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

        let executor_config = regs
            .agent_templates
            .get("@standard/goal-review-executor")
            .expect("executor installed")
            .definition
            .config
            .as_ref()
            .cloned()
            .expect("agent config");
        let referenced = executor_config
            .system_prompt_template_id
            .as_deref()
            .expect("template reference");
        assert_eq!(referenced, "@standard/goal-review-executor");
        assert_eq!(
            executor_config.system_prompt, None,
            "the prompt is referenced by id, never inlined"
        );

        // Resolution serves the override registered under the referenced id.
        let resolved = regs
            .templates
            .get(referenced)
            .expect("override registered")
            .content
            .clone();
        assert_eq!(resolved, "custom executor prompt");

        // The reviewer references the built-in prompt and has no override.
        let reviewer = regs
            .agent_templates
            .get("@standard/goal-review-reviewer")
            .expect("reviewer installed");
        let reviewer_ref = reviewer
            .definition
            .config
            .as_ref()
            .and_then(|c| c.system_prompt_template_id.as_deref())
            .expect("template reference");
        assert_eq!(reviewer_ref, "@standard/goal-review-reviewer");
        assert!(!regs.templates.has(reviewer_ref));
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
    fn config_field_types_match_json_shapes() {
        use serde_json::json;
        assert!(ResourceAssemblerConfigFieldType::String.matches_value(&json!("x")));
        assert!(!ResourceAssemblerConfigFieldType::String.matches_value(&json!(1)));
        assert!(ResourceAssemblerConfigFieldType::Number.matches_value(&json!(1)));
        assert!(ResourceAssemblerConfigFieldType::Boolean.matches_value(&json!(true)));
        assert!(ResourceAssemblerConfigFieldType::Array.matches_value(&json!([1])));
        assert!(ResourceAssemblerConfigFieldType::Object.matches_value(&json!({})));
        assert!(ResourceAssemblerConfigFieldType::Expression.matches_value(&json!("a + 1")));
        assert!(!ResourceAssemblerConfigFieldType::Expression.matches_value(&json!(1)));
    }

    #[test]
    fn metadata_gate_reports_required_and_types() {
        use serde_json::json;
        let metadata = ResourceAssemblerMetadata {
            id: "test".into(),
            name: "test".into(),
            version: "1".into(),
            description: "test".into(),
            author: None,
            tags: None,
            category: None,
            dependencies: None,
            configurable: Some(HashMap::from([(
                "count".into(),
                ResourceAssemblerConfigField {
                    r#type: ResourceAssemblerConfigFieldType::Number,
                    description: "count".into(),
                    default: None,
                    required: Some(true),
                    allowed_functions: None,
                    properties: None,
                    element_type: None,
                },
            )])),
        };
        assert!(validate_config_value_against_metadata(&json!({"count": 1}), &metadata).is_empty());
        let errors = validate_config_value_against_metadata(&json!({}), &metadata);
        assert!(errors.iter().any(|e| e.contains("count")));
        let errors = validate_config_value_against_metadata(&json!({"count": "many"}), &metadata);
        assert!(errors.iter().any(|e| e.contains("count")));
    }

    #[test]
    fn expression_allowlist_reports_every_unknown_call() {
        use serde_json::json;
        let metadata = ResourceAssemblerMetadata {
            id: "test".into(),
            name: "test".into(),
            version: "1".into(),
            description: "test".into(),
            author: None,
            tags: None,
            category: None,
            dependencies: None,
            configurable: Some(HashMap::from([(
                "when".into(),
                ResourceAssemblerConfigField {
                    r#type: ResourceAssemblerConfigFieldType::Expression,
                    description: "when".into(),
                    default: None,
                    required: None,
                    allowed_functions: Some(vec!["eq".into()]),
                    properties: None,
                    element_type: None,
                },
            )])),
        };
        assert!(
            validate_config_value_against_metadata(&json!({"when": "eq(a, 1)"}), &metadata)
                .is_empty()
        );
        let errors = validate_config_value_against_metadata(
            &json!({"when": "eq(a, 1) && hasRole(user) || \"call (x)\""}),
            &metadata,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("hasRole"), "{}", errors[0]);
        let errors = validate_config_value_against_metadata(
            &json!({"when": "nope(a) || other(b)"}),
            &metadata,
        );
        assert_eq!(errors.len(), 2);
        let open = ResourceAssemblerMetadata {
            configurable: Some(HashMap::from([(
                "when".into(),
                ResourceAssemblerConfigField {
                    r#type: ResourceAssemblerConfigFieldType::Expression,
                    description: "when".into(),
                    default: None,
                    required: None,
                    allowed_functions: None,
                    properties: None,
                    element_type: None,
                },
            )])),
            ..metadata
        };
        assert!(
            validate_config_value_against_metadata(&json!({"when": "any(a)"}), &open).is_empty()
        );
    }

    #[test]
    fn unknown_config_keys_are_reported_in_order() {
        use serde_json::json;
        let metadata = ResourceAssemblerMetadata {
            id: "test".into(),
            name: "test".into(),
            version: "1".into(),
            description: "test".into(),
            author: None,
            tags: None,
            category: None,
            dependencies: None,
            configurable: Some(HashMap::from([(
                "known".into(),
                ResourceAssemblerConfigField {
                    r#type: ResourceAssemblerConfigFieldType::String,
                    description: "known".into(),
                    default: None,
                    required: None,
                    allowed_functions: None,
                    properties: None,
                    element_type: None,
                },
            )])),
        };
        assert!(unknown_assembler_config_keys(&json!({"known": "x"}), &metadata).is_empty());
        assert!(unknown_assembler_config_keys(&json!(null), &metadata).is_empty());
        assert_eq!(
            unknown_assembler_config_keys(&json!({"known": "x", "zeta": 1, "alpha": 2}), &metadata),
            vec!["alpha".to_string(), "zeta".to_string()]
        );
    }

    #[test]
    fn strict_gate_rejects_unknown_missing_and_mismatch() {
        use serde_json::json;
        let metadata = ResourceAssemblerMetadata {
            id: "test".into(),
            name: "test".into(),
            version: "1".into(),
            description: "test".into(),
            author: None,
            tags: None,
            category: None,
            dependencies: None,
            configurable: Some(HashMap::from([(
                "count".into(),
                ResourceAssemblerConfigField {
                    r#type: ResourceAssemblerConfigFieldType::Number,
                    description: "count".into(),
                    default: None,
                    required: Some(true),
                    allowed_functions: None,
                    properties: None,
                    element_type: None,
                },
            )])),
        };
        assert!(gate_assembler_config(&json!({"count": 1}), &metadata).is_ok());
        let err = gate_assembler_config(&json!({"count": 1, "extra": 1}), &metadata)
            .expect_err("unknown keys fail");
        assert!(err.contains("extra"), "{err}");
        let err = gate_assembler_config(&json!({}), &metadata).expect_err("missing required fails");
        assert!(err.contains("count"), "{err}");
        let err = gate_assembler_config(&json!(null), &metadata)
            .expect_err("null reports missing required");
        assert!(err.contains("count"), "{err}");
        let err = gate_assembler_config(&json!({"count": "many"}), &metadata)
            .expect_err("type mismatch fails");
        assert!(err.contains("count"), "{err}");
    }

    #[test]
    fn nested_and_null_assembler_rules() {
        use serde_json::json;
        let metadata = ResourceAssemblerMetadata {
            id: "test".into(),
            name: "test".into(),
            version: "1".into(),
            description: "test".into(),
            author: None,
            tags: None,
            category: None,
            dependencies: None,
            configurable: Some(HashMap::from([
                (
                    "conn".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Object,
                        description: "conn".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                        properties: Some(HashMap::from([(
                            "port".into(),
                            ResourceAssemblerNestedField {
                                r#type: ResourceAssemblerConfigFieldType::Number,
                                required: Some(true),
                            },
                        )])),
                        element_type: None,
                    },
                ),
                (
                    "tags".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::Array,
                        description: "tags".into(),
                        default: None,
                        required: None,
                        allowed_functions: None,
                        properties: None,
                        element_type: Some(ResourceAssemblerConfigFieldType::String),
                    },
                ),
                (
                    "label".into(),
                    ResourceAssemblerConfigField {
                        r#type: ResourceAssemblerConfigFieldType::String,
                        description: "label".into(),
                        default: Some(json!("hi")),
                        required: None,
                        allowed_functions: None,
                        properties: None,
                        element_type: None,
                    },
                ),
            ])),
        };
        let errors = validate_config_value_against_metadata(
            &json!({"conn": {"port": "many"}, "tags": ["a", 1]}),
            &metadata,
        );
        assert!(errors.iter().any(|e| e.contains("conn.port")), "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("tags[1]")), "{errors:?}");
        assert!(validate_config_value_against_metadata(
            &json!({"conn": {"port": 1}, "tags": [], "label": null}),
            &metadata,
        )
        .is_empty());
        let filled = apply_assembler_config_defaults(&json!({"label": null}), &metadata);
        assert_eq!(filled["label"], "hi");
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
