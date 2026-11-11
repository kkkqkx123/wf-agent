use super::*;

fn template_with_nodes() -> WorkflowTemplate {
    use wf_types::node::BaseStaticNode;
    use wf_types::workflow::{
        Edge, TriggeredSubworkflowConfig, WorkflowDefinition, WorkflowMetadata,
    };
    WorkflowTemplate {
        id: "t_flow".to_string(),
        name: "T Flow".to_string(),
        description: "test".to_string(),
        definition: WorkflowDefinition {
            id: "t_flow".to_string(),
            name: "T Flow".to_string(),
            description: Some("test".to_string()),
            r#type: None,
            version: None,
            nodes: vec![
                BaseStaticNode {
                    id: "start".into(),
                    node_type: StaticNodeType::StartFromMessage,
                    name: Some("Start".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "llm".into(),
                    node_type: StaticNodeType::Llm,
                    name: Some("LLM".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
                BaseStaticNode {
                    id: "end".into(),
                    node_type: StaticNodeType::ContinueFromMessage,
                    name: Some("End".into()),
                    description: None,
                    config: None,
                    execution_config: None,
                },
            ],
            edges: vec![
                Edge {
                    id: "e1".into(),
                    source_node_id: "start".into(),
                    target_node_id: "llm".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                    error_route: None,
                },
                Edge {
                    id: "e2".into(),
                    source_node_id: "llm".into(),
                    target_node_id: "end".into(),
                    r#type: EdgeType::Default,
                    condition: None,
                    label: None,
                    description: None,
                    weight: None,
                    metadata: None,
                    error_route: None,
                },
            ],
            config: None,
            variables: None,
            input_strict: None,
            triggered_subworkflow_config: Some(TriggeredSubworkflowConfig {
                enable_checkpoints: Some(false),
                timeout: Some(5000),
                compression_fallback: None,
            }),
            metadata: Some(WorkflowMetadata {
                author: None,
                tags: None,
                category: None,
                ..Default::default()
            }),
            available_tools: None,
            hooks: None,
            layout: None,
            created_at: 0,
            updated_at: 0,
        },
        template_category: None,
        template_tags: None,
        is_public: None,
        enabled: None,
        is_system: None,
        is_published: None,
    }
}

#[test]
fn template_to_graph_maps_nodes_edges_and_endpoints() {
    let template = template_with_nodes();
    let graph = template_to_graph(&template);

    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.nodes[0].node_type, "START_FROM_MESSAGE");
    assert_eq!(graph.nodes[1].node_type, "LLM");
    assert_eq!(graph.nodes[2].node_type, "CONTINUE_FROM_MESSAGE");
    assert_eq!(graph.start_node_id.as_deref(), Some("start"));
    assert_eq!(graph.end_node_ids, vec!["end".to_string()]);
    assert_eq!(graph.edges.len(), 2);
    assert_eq!(graph.edges[0].source_node_id, "start");
    assert_eq!(graph.edges[1].target_node_id, "end");
}
