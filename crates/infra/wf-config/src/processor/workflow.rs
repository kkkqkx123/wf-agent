use crate::error::{ConfigError, ConfigResult};
use crate::processor::hook::validate_base_hook_config;
use crate::processor::tool_list::validate_available_tools;
use crate::validator::{validate_min, validate_required};

use wf_types::node::r#static::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::definition::{WorkflowDefinition, WorkflowDefinitionType};
use wf_types::workflow::edge::{Edge, EdgeType};

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

    if let Some(r#type) = definition.r#type.as_ref() {
        if *r#type == WorkflowDefinitionType::TriggeredSubworkflow
            && definition.triggered_subworkflow_config.is_none()
        {
            return Err(ConfigError::Validation(
                "workflow type is TriggeredSubworkflow but triggered_subworkflow_config is missing"
                    .into(),
            ));
        }
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
    if let Some(max_retries) = config.max_retries {
        validate_min(max_retries, 0, "triggered_subworkflow_config.max_retries")?;
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
            })
        })
        .collect()
}

fn parse_node_type(type_str: &str, node_id: &str) -> ConfigResult<StaticNodeType> {
    match type_str.to_uppercase().as_str() {
        "START" => Ok(StaticNodeType::Start),
        "END" => Ok(StaticNodeType::End),
        "VARIABLE" => Ok(StaticNodeType::Variable),
        "FORK" => Ok(StaticNodeType::Fork),
        "JOIN" => Ok(StaticNodeType::Join),
        "SYNC" => Ok(StaticNodeType::Sync),
        "SUBGRAPH" => Ok(StaticNodeType::Subgraph),
        "EMBED_GRAPH" => Ok(StaticNodeType::EmbedGraph),
        "SCRIPT" => Ok(StaticNodeType::Script),
        "INTERACTIVE_SCRIPT" => Ok(StaticNodeType::InteractiveScript),
        "LLM" => Ok(StaticNodeType::Llm),
        "TOOL_VISIBILITY" => Ok(StaticNodeType::ToolVisibility),
        "USER_INTERACTION" => Ok(StaticNodeType::UserInteraction),
        "ROUTE" => Ok(StaticNodeType::Route),
        "CONTEXT_PROCESSOR" => Ok(StaticNodeType::ContextProcessor),
        "LOOP_START" => Ok(StaticNodeType::LoopStart),
        "LOOP_END" => Ok(StaticNodeType::LoopEnd),
        "AGENT_LOOP" => Ok(StaticNodeType::AgentLoop),
        "START_FROM_MESSAGE" => Ok(StaticNodeType::StartFromMessage),
        "CONTINUE_FROM_MESSAGE" => Ok(StaticNodeType::ContinueFromMessage),
        "EMBED_START" => Ok(StaticNodeType::EmbedStart),
        "EMBED_END" => Ok(StaticNodeType::EmbedEnd),
        _ => Err(ConfigError::Validation(format!(
            "node '{node_id}' has unknown node type '{type_str}'"
        ))),
    }
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
    pub condition: Option<String>,
    pub label: Option<String>,
    pub description: Option<String>,
    pub weight: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workflow() -> WorkflowDefinition {
        WorkflowDefinition {
            id: "wf-1".to_string(),
            name: "Test Workflow".to_string(),
            description: None,
            r#type: None,
            version: None,
            nodes: vec![BaseStaticNode {
                id: "node-1".to_string(),
                node_type: StaticNodeType::Start,
                name: None,
                description: None,
                config: None,
                execution_config: None,
            }],
            edges: vec![],
            config: None,
            variables: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            available_tools: None,
            hooks: None,
        }
    }

    #[test]
    fn test_valid_workflow() {
        let wf = make_workflow();
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_empty_nodes() {
        let mut wf = make_workflow();
        wf.nodes = vec![];
        assert!(validate_workflow_definition(&wf).is_err());
    }

    #[test]
    fn test_empty_id() {
        let mut wf = make_workflow();
        wf.id = String::new();
        assert!(validate_workflow_definition(&wf).is_err());
    }

    #[test]
    fn test_unknown_hook_type_allowed_with_warning() {
        let mut wf = make_workflow();
        wf.hooks = Some(vec![wf_types::hook::BaseHookConfig {
            hook_type: "BEFORE_ECECUTE".to_string(),
            condition: None,
            event_name: "e".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: None,
        }]);
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_known_hook_type_accepted() {
        let mut wf = make_workflow();
        wf.hooks = Some(vec![wf_types::hook::BaseHookConfig {
            hook_type: "WORKFLOW_BEFORE".to_string(),
            condition: None,
            event_name: "e".to_string(),
            event_payload: None,
            enabled: Some(true),
            weight: None,
            create_checkpoint: None,
            checkpoint_description: None,
            receiver: None,
        }]);
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_transform_nodes() {
        let configs = vec![
            WorkflowNodeConfig {
                id: "n1".to_string(),
                node_type: "LLM".to_string(),
                name: Some("My LLM".to_string()),
                description: None,
                config: None,
            },
            WorkflowNodeConfig {
                id: "n2".to_string(),
                node_type: "SCRIPT".to_string(),
                name: None,
                description: None,
                config: None,
            },
        ];

        let nodes = transform_nodes(&configs).expect("known types transform");
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].id, "n1");
        assert_eq!(nodes[0].name, Some("My LLM".to_string()));
        assert_eq!(nodes[1].id, "n2");
        assert_eq!(nodes[1].name, Some("n2".to_string()));
        assert_eq!(nodes[0].node_type, StaticNodeType::Llm);
        assert_eq!(nodes[1].node_type, StaticNodeType::Script);
    }

    #[test]
    fn test_transform_edges() {
        let configs = vec![
            WorkflowEdgeConfig {
                id: None,
                source_node_id: Some("n1".to_string()),
                target_node_id: Some("n2".to_string()),
                condition: None,
                label: None,
                description: None,
                weight: None,
            },
            WorkflowEdgeConfig {
                id: Some("e2".to_string()),
                source_node_id: Some("n2".to_string()),
                target_node_id: Some("n3".to_string()),
                condition: Some("success".to_string()),
                label: None,
                description: None,
                weight: Some(1),
            },
        ];

        let edges = transform_edges(&configs).expect("valid edges transform");
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].r#type, EdgeType::Default);
        assert_eq!(edges[1].r#type, EdgeType::Conditional);
        assert_eq!(edges[1].id, "e2".to_string());
    }

    #[test]
    fn test_transform_nodes_rejects_unknown_type() {
        let configs = vec![WorkflowNodeConfig {
            id: "n1".to_string(),
            node_type: "LLMM".to_string(),
            name: None,
            description: None,
            config: None,
        }];
        let err = transform_nodes(&configs).unwrap_err();
        assert!(err.to_string().contains("unknown node type"));
    }

    #[test]
    fn test_transform_edges_rejects_missing_endpoints() {
        let configs = vec![WorkflowEdgeConfig {
            id: None,
            source_node_id: Some("n1".to_string()),
            target_node_id: None,
            condition: None,
            label: None,
            description: None,
            weight: None,
        }];
        let err = transform_edges(&configs).unwrap_err();
        assert!(err.to_string().contains("target_node_id"));
    }

    #[test]
    fn test_workflow_config_timeout_validation() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: Some(0),
            max_steps: None,
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: Some(5000),
            max_steps: None,
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_workflow_config_max_steps_validation() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: Some(0),
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: Some(100),
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_workflow_config_retry_policy_validation() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: Some(wf_types::execution::RetryPolicy {
                enabled: true,
                max_retries: 3,
                base_delay_ms: 1000,
                max_delay_ms: Some(500),
                backoff_multiplier: None,
                jitter: None,
            }),
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: Some(wf_types::execution::RetryPolicy {
                enabled: true,
                max_retries: 3,
                base_delay_ms: 1000,
                max_delay_ms: Some(5000),
                backoff_multiplier: Some(0.5),
                jitter: None,
            }),
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: Some(wf_types::execution::RetryPolicy {
                enabled: true,
                max_retries: 3,
                base_delay_ms: 1000,
                max_delay_ms: Some(5000),
                backoff_multiplier: Some(2.0),
                jitter: None,
            }),
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_triggered_subworkflow_config_validation() {
        let mut wf = make_workflow();
        wf.triggered_subworkflow_config =
            Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
                enable_checkpoints: None,
                timeout: Some(0),
                max_retries: None,
            });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.triggered_subworkflow_config =
            Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
                enable_checkpoints: None,
                timeout: Some(5000),
                max_retries: Some(3),
            });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_workflow_variables_validation() {
        let mut wf = make_workflow();
        wf.variables = Some(vec![wf_types::workflow_execution::VariableDefinition {
            name: "9bad".to_string(),
            value: serde_json::json!(null),
            r#type: None,
            scope: None,
            readonly: None,
            metadata: None,
        }]);
        assert!(validate_workflow_definition(&wf).is_err());

        wf.variables = Some(vec![wf_types::workflow_execution::VariableDefinition {
            name: "valid_name".to_string(),
            value: serde_json::json!(null),
            r#type: None,
            scope: None,
            readonly: None,
            metadata: None,
        }]);
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_edge_weight_validation() {
        let mut wf = make_workflow();
        wf.nodes.push(BaseStaticNode {
            id: "node-2".to_string(),
            node_type: StaticNodeType::End,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        });
        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-2".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: Some(0),
            metadata: None,
        }];
        assert!(validate_workflow_definition(&wf).is_err());

        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-2".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: Some(5),
            metadata: None,
        }];
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_edge_references_unknown_source_node_deferred_to_engine() {
        let mut wf = make_workflow();
        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "nonexistent".to_string(),
            target_node_id: "node-1".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        }];
        // Edge existence is owned by GraphValidator; config layer only checks shape.
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_edge_references_unknown_target_node_deferred_to_engine() {
        let mut wf = make_workflow();
        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "nonexistent".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        }];
        // Edge existence is owned by GraphValidator; config layer only checks shape.
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_edge_valid_references_accepted() {
        let mut wf = make_workflow();
        wf.nodes.push(BaseStaticNode {
            id: "node-2".to_string(),
            node_type: StaticNodeType::End,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        });
        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-2".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        }];
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_cycle_detection_deferred_to_engine_self_loop() {
        let mut wf = make_workflow();
        wf.edges = vec![wf_types::workflow::edge::Edge {
            id: "e1".to_string(),
            source_node_id: "node-1".to_string(),
            target_node_id: "node-1".to_string(),
            r#type: wf_types::workflow::edge::EdgeType::Default,
            condition: None,
            label: None,
            description: None,
            weight: None,
            metadata: None,
        }];
        // Cycle detection is owned by GraphValidator; config layer allows it.
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_cycle_detection_deferred_to_engine_two_node_cycle() {
        let mut wf = make_workflow();
        wf.nodes.push(BaseStaticNode {
            id: "node-2".to_string(),
            node_type: StaticNodeType::Llm,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        });
        wf.edges = vec![
            wf_types::workflow::edge::Edge {
                id: "e1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                r#type: wf_types::workflow::edge::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            },
            wf_types::workflow::edge::Edge {
                id: "e2".to_string(),
                source_node_id: "node-2".to_string(),
                target_node_id: "node-1".to_string(),
                r#type: wf_types::workflow::edge::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            },
        ];
        // Cycle detection is owned by GraphValidator; config layer allows it.
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_dag_accepted() {
        let mut wf = make_workflow();
        wf.nodes.push(BaseStaticNode {
            id: "node-2".to_string(),
            node_type: StaticNodeType::Sync,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        });
        wf.nodes.push(BaseStaticNode {
            id: "node-3".to_string(),
            node_type: StaticNodeType::End,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        });
        wf.edges = vec![
            wf_types::workflow::edge::Edge {
                id: "e1".to_string(),
                source_node_id: "node-1".to_string(),
                target_node_id: "node-2".to_string(),
                r#type: wf_types::workflow::edge::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            },
            wf_types::workflow::edge::Edge {
                id: "e2".to_string(),
                source_node_id: "node-2".to_string(),
                target_node_id: "node-3".to_string(),
                r#type: wf_types::workflow::edge::EdgeType::Default,
                condition: None,
                label: None,
                description: None,
                weight: None,
                metadata: None,
            },
        ];
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_triggered_subworkflow_type_requires_config() {
        let mut wf = make_workflow();
        wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
        wf.triggered_subworkflow_config = None;
        let err = validate_workflow_definition(&wf).unwrap_err();
        assert!(err
            .to_string()
            .contains("triggered_subworkflow_config is missing"));
    }

    #[test]
    fn test_triggered_subworkflow_type_with_config_accepted() {
        let mut wf = make_workflow();
        wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
        wf.triggered_subworkflow_config =
            Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
                enable_checkpoints: None,
                timeout: None,
                max_retries: None,
            });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_non_triggered_type_without_config_accepted() {
        let mut wf = make_workflow();
        wf.r#type = Some(WorkflowDefinitionType::Standalone);
        wf.triggered_subworkflow_config = None;
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_available_tools_intersection_at_definition_level() {
        let mut wf = make_workflow();
        wf.available_tools = Some(wf_types::tool::AvailableTools {
            available: vec!["tool_a".to_string()],
            initial: None,
            discoverable: None,
            hidden: Some(vec!["tool_a".to_string()]),
            enable_general_tool: None,
            require_approval: None,
            allowed_workflows: None,
        });
        let err = validate_workflow_definition(&wf).unwrap_err();
        assert!(err.to_string().contains("must not intersect"));
    }

    #[test]
    fn test_available_tools_no_intersection_accepted() {
        let mut wf = make_workflow();
        wf.available_tools = Some(wf_types::tool::AvailableTools {
            available: vec!["tool_a".to_string()],
            initial: None,
            discoverable: Some(vec!["tool_b".to_string()]),
            hidden: Some(vec!["tool_c".to_string()]),
            enable_general_tool: None,
            require_approval: None,
            allowed_workflows: None,
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_available_tools_intersection_at_config_level() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: Some(wf_types::tool::AvailableTools {
                available: vec!["tool_a".to_string()],
                initial: None,
                discoverable: None,
                hidden: Some(vec!["tool_a".to_string()]),
                enable_general_tool: None,
                require_approval: None,
                allowed_workflows: None,
            }),
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: None,
        });
        let err = validate_workflow_definition(&wf).unwrap_err();
        assert!(err.to_string().contains("must not intersect"));
    }

    #[test]
    fn test_static_contexts_must_be_objects() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: Some(vec![serde_json::json!("not an object")]),
        });
        let err = validate_workflow_definition(&wf).unwrap_err();
        assert!(err.to_string().contains("must be a JSON object"));
    }

    #[test]
    fn test_static_contexts_valid_objects_accepted() {
        let mut wf = make_workflow();
        wf.config = Some(wf_types::workflow::WorkflowConfig {
            timeout: None,
            max_steps: None,
            checkpoint: None,
            retry_policy: None,
            tool_approval: None,
            available_tools: None,
            initial_messages: None,
            system_prompt_template_id: None,
            system_prompt_template_variables: None,
            system_prompt: None,
            static_contexts: Some(vec![
                serde_json::json!({"key": "value"}),
                serde_json::json!({"nested": {"a": 1}}),
            ]),
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }
}
