use crate::error::{ConfigError, ConfigResult};
use crate::validator::validate_required;

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

    Ok(())
}

pub fn transform_nodes(nodes: &[WorkflowNodeConfig]) -> Vec<BaseStaticNode> {
    nodes
        .iter()
        .map(|node| BaseStaticNode {
            id: node.id.clone(),
            node_type: parse_node_type(&node.node_type),
            name: Some(node.name.as_deref().unwrap_or(&node.id).to_string()),
            description: node.description.clone(),
            config: node.config.clone(),
            execution_config: None,
        })
        .collect()
}

pub fn transform_edges(edges: &[WorkflowEdgeConfig]) -> Vec<Edge> {
    edges
        .iter()
        .map(|edge| {
            let has_condition = edge.condition.is_some();
            Edge {
                id: edge.id.clone().unwrap_or_else(generate_edge_id),
                source_node_id: edge.source_node_id.clone().unwrap_or_default(),
                target_node_id: edge.target_node_id.clone().unwrap_or_default(),
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
            }
        })
        .collect()
}

fn parse_node_type(type_str: &str) -> StaticNodeType {
    match type_str.to_uppercase().as_str() {
        "START" => StaticNodeType::Start,
        "END" => StaticNodeType::End,
        "VARIABLE" => StaticNodeType::Variable,
        "FORK" => StaticNodeType::Fork,
        "JOIN" => StaticNodeType::Join,
        "SYNC" => StaticNodeType::Sync,
        "SUBGRAPH" => StaticNodeType::Subgraph,
        "EMBED_GRAPH" => StaticNodeType::EmbedGraph,
        "SCRIPT" => StaticNodeType::Script,
        "INTERACTIVE_SCRIPT" => StaticNodeType::InteractiveScript,
        "LLM" => StaticNodeType::Llm,
        "TOOL_VISIBILITY" => StaticNodeType::ToolVisibility,
        "USER_INTERACTION" => StaticNodeType::UserInteraction,
        "ROUTE" => StaticNodeType::Route,
        "CONTEXT_PROCESSOR" => StaticNodeType::ContextProcessor,
        "LOOP_START" => StaticNodeType::LoopStart,
        "LOOP_END" => StaticNodeType::LoopEnd,
        "AGENT_LOOP" => StaticNodeType::AgentLoop,
        "START_FROM_TRIGGER" => StaticNodeType::StartFromTrigger,
        "CONTINUE_FROM_TRIGGER" => StaticNodeType::ContinueFromTrigger,
        _ => StaticNodeType::Llm,
    }
}

fn generate_edge_id() -> String {
    wf_common::generate_id()
}

#[derive(Debug, Clone)]
pub struct WorkflowNodeConfig {
    pub id: String,
    pub node_type: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub config: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
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
            triggers: None,
            triggered_subworkflow_config: None,
            metadata: None,
            created_at: 0,
            updated_at: 0,
            available_tools: None,
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

        let nodes = transform_nodes(&configs);
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

        let edges = transform_edges(&configs);
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].r#type, EdgeType::Default);
        assert_eq!(edges[1].r#type, EdgeType::Conditional);
        assert_eq!(edges[1].id, "e2".to_string());
    }
}
