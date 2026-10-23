use crate::error::{ConfigError, ConfigResult};
use crate::processor::hook::validate_base_hook_config;
use crate::processor::tool_list::validate_available_tools;
use crate::validator::{validate_min, validate_required};

use wf_types::node::r#static::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::definition::{WorkflowDefinition, WorkflowDefinitionType};
use wf_types::workflow::edge::{Edge, EdgeType};
use wf_types::workflow::error_branch::ErrorRouteConfig;

fn declared_type_name(r#type: Option<&WorkflowDefinitionType>) -> &'static str {
    match r#type {
        None => "missing",
        Some(WorkflowDefinitionType::Standalone) => "Standalone",
        Some(WorkflowDefinitionType::Dependent) => "Dependent",
        Some(WorkflowDefinitionType::TriggeredSubworkflow) => "TriggeredSubworkflow",
    }
}

pub fn validate_workflow_definition(definition: &WorkflowDefinition) -> ConfigResult<()> {
    validate_required(&definition.id, "id")?;
    validate_required(&definition.name, "name")?;

    if definition.nodes.is_empty() {
        return Err(ConfigError::Validation(
            "workflow must have at least one node".into(),
        ));
    }

    for node in &definition.nodes {
        validate_required(&node.id, "node.id")?;
    }

    for edge in &definition.edges {
        validate_required(&edge.id, "edge.id")?;
        validate_required(&edge.source_node_id, "edge.source_node_id")?;
        validate_required(&edge.target_node_id, "edge.target_node_id")?;
        if let Some(weight) = edge.weight {
            validate_min(weight, 1, "edge.weight")?;
        }
        // Edge source/target existence is validated by GraphValidator in the
        // engine layer (single source of truth for graph semantics).
    }

    // Cycle detection is performed by GraphValidator in the engine layer
    // (which correctly excludes legal LOOP_END -> LOOP_START control edges).
    // Removed here to avoid redundant work and divergent semantics.

    // The definition type is an executability contract, not a label:
    // `triggered_subworkflow_config` is owned exclusively by
    // `TriggeredSubworkflow`, and a child-only `Dependent` workflow promises
    // plain `Start`/`End` boundaries because message-channel nodes belong to
    // the triggered/compression mechanism.
    if let Some(r#type) = definition.r#type.as_ref() {
        if *r#type == WorkflowDefinitionType::TriggeredSubworkflow
            && definition.triggered_subworkflow_config.is_none()
        {
            return Err(ConfigError::Validation(
                "workflow type is TriggeredSubworkflow but triggered_subworkflow_config is missing"
                    .into(),
            ));
        }
        if *r#type != WorkflowDefinitionType::TriggeredSubworkflow
            && definition.triggered_subworkflow_config.is_some()
        {
            return Err(ConfigError::Validation(
                format!(
                    "triggered_subworkflow_config is present but workflow type is {}, expected TriggeredSubworkflow",
                    declared_type_name(Some(r#type)),
                ),
            ));
        }
    } else if definition.triggered_subworkflow_config.is_some() {
        return Err(ConfigError::Validation(
            "triggered_subworkflow_config is present but workflow type is missing, expected TriggeredSubworkflow"
                .into(),
        ));
    }

    if matches!(definition.r#type, Some(WorkflowDefinitionType::Dependent))
        && definition.nodes.iter().any(|node| {
            matches!(
                node.node_type,
                StaticNodeType::StartFromMessage | StaticNodeType::ContinueFromMessage
            )
        })
    {
        return Err(ConfigError::Validation(
            "workflow type is Dependent but the graph contains message-channel nodes (StartFromMessage/ContinueFromMessage); message-channel children must declare TriggeredSubworkflow"
                .into(),
        ));
    }

    // Type-topology consistency gate: the declared type states the boundary
    // category, the node list states the fact. Both must agree on the
    // category; exact counts and connectivity stay owned by the engine graph
    // validator, which is the single source for topology details.
    let has_message_nodes = definition.nodes.iter().any(|node| {
        matches!(
            node.node_type,
            StaticNodeType::StartFromMessage | StaticNodeType::ContinueFromMessage
        )
    });
    let has_plain_boundary = definition
        .nodes
        .iter()
        .any(|node| matches!(node.node_type, StaticNodeType::Start | StaticNodeType::End));

    if matches!(
        definition.r#type,
        Some(WorkflowDefinitionType::TriggeredSubworkflow)
    ) {
        if !has_message_nodes {
            return Err(ConfigError::Validation(
                "workflow type is TriggeredSubworkflow but the graph contains no message-channel nodes (StartFromMessage/ContinueFromMessage)"
                    .into(),
            ));
        }
        if has_plain_boundary {
            return Err(ConfigError::Validation(
                "workflow type is TriggeredSubworkflow but the graph contains plain START/END nodes; triggered graphs must use message-channel boundaries only"
                    .into(),
            ));
        }
    }

    if matches!(definition.r#type, Some(WorkflowDefinitionType::Standalone)) && has_message_nodes {
        return Err(ConfigError::Validation(
            "workflow type is Standalone but the graph contains message-channel nodes (StartFromMessage/ContinueFromMessage); message-channel children must declare TriggeredSubworkflow"
                .into(),
        ));
    }

    // A missing declaration behaves like Standalone for message-channel
    // topology: leaving the type out must not bypass the gate above.
    if definition.r#type.is_none() && has_message_nodes {
        return Err(ConfigError::Validation(
            "workflow type is missing but the graph contains message-channel nodes (StartFromMessage/ContinueFromMessage); message-channel children must declare TriggeredSubworkflow"
                .into(),
        ));
    }

    if let Some(hooks) = definition.hooks.as_ref() {
        for (idx, hook) in hooks.iter().enumerate() {
            // Unknown hook types produce warnings, not errors, to allow
            // forward-compatible definitions (see validate_base_hook_config).
            validate_base_hook_config(hook, &format!("definition.hooks[{idx}]"))?;
        }
    }

    if let Some(ref config) = definition.config {
        validate_workflow_config(config)?;
    }

    if let Some(ref triggered) = definition.triggered_subworkflow_config {
        validate_triggered_subworkflow_config(triggered)?;
    }

    if let Some(ref variables) = definition.variables {
        validate_workflow_variables(variables)?;
    }

    validate_workflow_available_tools_intersection(definition)?;

    // Node config validation is performed by GraphValidator in the engine
    // layer via the shared node_config validators. Removed here to avoid
    // redundant work.

    Ok(())
}

pub fn validate_workflow_config(config: &wf_types::workflow::WorkflowConfig) -> ConfigResult<()> {
    if let Some(timeout) = config.timeout {
        validate_min(timeout, 1, "config.timeout")?;
    }
    if let Some(max_steps) = config.max_steps {
        validate_min(max_steps, 1, "config.max_steps")?;
    }
    if let Some(ref retry_policy) = config.retry_policy {
        validate_retry_policy(retry_policy)?;
    }
    if let Some(ref static_contexts) = config.static_contexts {
        for (idx, ctx) in static_contexts.iter().enumerate() {
            if !ctx.is_object() {
                let type_name = match ctx {
                    serde_json::Value::Null => "null",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Object(_) => unreachable!(),
                };
                return Err(ConfigError::Validation(format!(
                    "config.static_contexts[{idx}] must be a JSON object, got {type_name}"
                )));
            }
        }
    }
    Ok(())
}

fn validate_workflow_available_tools_intersection(
    definition: &WorkflowDefinition,
) -> ConfigResult<()> {
    if let Some(ref tools) = definition.available_tools {
        validate_available_tools(tools, "definition.available_tools")?;
    }
    if let Some(ref config) = definition.config {
        if let Some(ref tools) = config.available_tools {
            validate_available_tools(tools, "config.available_tools")?;
        }
    }
    Ok(())
}

fn validate_retry_policy(policy: &wf_types::execution::RetryPolicy) -> ConfigResult<()> {
    validate_min(policy.max_retries, 0, "config.retry_policy.max_retries")?;
    validate_min(policy.base_delay_ms, 1, "config.retry_policy.base_delay_ms")?;
    if let Some(max_delay) = policy.max_delay_ms {
        validate_min(max_delay, 1, "config.retry_policy.max_delay_ms")?;
        if max_delay < policy.base_delay_ms {
            return Err(ConfigError::Validation(format!(
                "config.retry_policy.max_delay_ms ({}) must be >= base_delay_ms ({})",
                max_delay, policy.base_delay_ms
            )));
        }
    }
    if let Some(multiplier) = policy.backoff_multiplier {
        if multiplier < 1.0 {
            return Err(ConfigError::Validation(format!(
                "config.retry_policy.backoff_multiplier must be >= 1.0, got {multiplier}"
            )));
        }
    }
    Ok(())
}

fn validate_triggered_subworkflow_config(
    config: &wf_types::workflow::definition::TriggeredSubworkflowConfig,
) -> ConfigResult<()> {
    if let Some(timeout) = config.timeout {
        validate_min(timeout, 1, "triggered_subworkflow_config.timeout")?;
    }
    Ok(())
}

fn validate_workflow_variables(
    variables: &[wf_types::workflow_execution::VariableDefinition],
) -> ConfigResult<()> {
    for (idx, var) in variables.iter().enumerate() {
        validate_required(&var.name, &format!("variables[{}].name", idx))?;
        if !crate::processor::node_config::is_valid_identifier(&var.name) {
            return Err(ConfigError::Validation(format!(
                "variables[{}].name '{}' must start with a letter or '_' and contain only letters, digits or '_'",
                idx, var.name
            )));
        }
    }
    Ok(())
}

pub fn transform_nodes(nodes: &[WorkflowNodeConfig]) -> ConfigResult<Vec<BaseStaticNode>> {
    nodes
        .iter()
        .map(|node| {
            Ok(BaseStaticNode {
                id: node.id.clone(),
                node_type: parse_node_type(&node.node_type, &node.id)?,
                name: Some(node.name.as_deref().unwrap_or(&node.id).to_string()),
                description: node.description.clone(),
                config: node.config.clone(),
                execution_config: None,
            })
        })
        .collect()
}

pub fn transform_edges(edges: &[WorkflowEdgeConfig]) -> ConfigResult<Vec<Edge>> {
    edges
        .iter()
        .enumerate()
        .map(|(idx, edge)| {
            let source_node_id = edge
                .source_node_id
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ConfigError::Validation(format!(
                        "edge at index {idx} is missing required source_node_id"
                    ))
                })?;
            let target_node_id = edge
                .target_node_id
                .clone()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    ConfigError::Validation(format!(
                        "edge at index {idx} is missing required target_node_id"
                    ))
                })?;
            let declared = edge
                .edge_type
                .as_deref()
                .map(|v| v.trim().to_uppercase())
                .filter(|v| !v.is_empty());
            if let Some(kind) = declared.as_deref() {
                if kind == "ERROR" {
                    if edge.condition.is_some() {
                        return Err(ConfigError::Validation(format!(
                            "edge at index {idx} declares ERROR but carries a condition; failure routes must not carry conditions"
                        )));
                    }
                    return Ok(Edge {
                        id: edge.id.clone().unwrap_or_else(generate_edge_id),
                        source_node_id,
                        target_node_id,
                        r#type: EdgeType::Error,
                        condition: None,
                        label: edge.label.clone(),
                        description: edge.description.clone(),
                        weight: edge.weight,
                        metadata: None,
                        error_route: edge.error_route.clone(),
                    });
                }
                if kind != "DEFAULT" && kind != "CONDITIONAL" {
                    return Err(ConfigError::Validation(format!(
                        "edge at index {idx} declares unknown edge type '{kind}'; expected DEFAULT, CONDITIONAL or ERROR"
                    )));
                }
            }
            if edge.error_route.is_some() {
                return Err(ConfigError::Validation(format!(
                    "edge at index {idx} carries error_route but is not declared ERROR"
                )));
            }
            let has_condition = edge.condition.is_some();
            Ok(Edge {
                id: edge.id.clone().unwrap_or_else(generate_edge_id),
                source_node_id,
                target_node_id,
                r#type: if has_condition {
                    EdgeType::Conditional
                } else {
                    EdgeType::Default
                },
                condition: edge.condition.clone(),
                label: edge.label.clone(),
                description: edge.description.clone(),
                weight: edge.weight,
                metadata: None,
                error_route: None,
            })
        })
        .collect()
}

fn parse_node_type(type_str: &str, node_id: &str) -> ConfigResult<StaticNodeType> {
    if type_str.trim().is_empty() {
        return Err(ConfigError::Validation(format!(
            "node '{node_id}' has a blank node_type"
        )));
    }
    // Same rule as `StaticNodeType`'s deserialization: builtin names win
    // (matched case-insensitively), anything else is a plugin-contributed type
    // kept verbatim so it round-trips. Whether a plugin actually registered the
    // name is decided at execution time by handler resolution.
    Ok(StaticNodeType::from_str_ci(type_str)
        .unwrap_or_else(|| StaticNodeType::Custom(type_str.to_string())))
}

fn generate_edge_id() -> String {
    wf_common::generate_id()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowNodeConfig {
    pub id: String,
    pub node_type: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkflowEdgeConfig {
    pub id: Option<String>,
    pub source_node_id: Option<String>,
    pub target_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<String>,
    pub condition: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub weight: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_route: Option<ErrorRouteConfig>,
}

#[cfg(test)]
#[path = "workflow/tests.rs"]
mod tests;
#[cfg(test)]
#[path = "workflow/tests_b.rs"]
mod tests_b;
