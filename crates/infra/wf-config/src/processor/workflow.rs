use crate::error::{ConfigError, ConfigResult};
use crate::processor::node_config;
use crate::validator::{validate_hook_type, validate_required};

use wf_types::node::r#static::{BaseStaticNode, StaticNodeType};
use wf_types::workflow::definition::WorkflowDefinition;
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
    }

    if let Some(hooks) = definition.hooks.as_ref() {
        for hook in hooks {
            validate_hook_type(&hook.hook_type, "definition.hooks")?;
        }
    }

    let mut issues = Vec::new();
    for node in &definition.nodes {
        issues.extend(node_config::validate_node_config(
            &node_type_name(&node.node_type),
            &node.id,
            node.config.as_ref(),
        ));
    }
    if !issues.is_empty() {
        let details = issues
            .iter()
            .map(|i| format!("{}: {}", i.field, i.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(ConfigError::Validation(details));
    }

    Ok(())
}

fn node_type_name(node_type: &StaticNodeType) -> String {
    match node_type {
        StaticNodeType::Start => "START".to_string(),
        StaticNodeType::End => "END".to_string(),
        StaticNodeType::EmbedStart => "EMBED_START".to_string(),
        StaticNodeType::EmbedEnd => "EMBED_END".to_string(),
        StaticNodeType::Variable => "VARIABLE".to_string(),
        StaticNodeType::Fork => "FORK".to_string(),
        StaticNodeType::Join => "JOIN".to_string(),
        StaticNodeType::Sync => "SYNC".to_string(),
        StaticNodeType::Subgraph => "SUBGRAPH".to_string(),
        StaticNodeType::EmbedGraph => "EMBED_GRAPH".to_string(),
        StaticNodeType::Script => "SCRIPT".to_string(),
        StaticNodeType::InteractiveScript => "INTERACTIVE_SCRIPT".to_string(),
        StaticNodeType::Llm => "LLM".to_string(),
        StaticNodeType::ToolVisibility => "TOOL_VISIBILITY".to_string(),
        StaticNodeType::UserInteraction => "USER_INTERACTION".to_string(),
        StaticNodeType::Route => "ROUTE".to_string(),
        StaticNodeType::ContextProcessor => "CONTEXT_PROCESSOR".to_string(),
        StaticNodeType::LoopStart => "LOOP_START".to_string(),
        StaticNodeType::LoopEnd => "LOOP_END".to_string(),
        StaticNodeType::AgentLoop => "AGENT_LOOP".to_string(),
        StaticNodeType::StartFromMessage => "START_FROM_MESSAGE".to_string(),
        StaticNodeType::ContinueFromMessage => "CONTINUE_FROM_MESSAGE".to_string(),
    }
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
    fn test_unknown_hook_type_rejected() {
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
        assert!(validate_workflow_definition(&wf).is_err());
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
}
