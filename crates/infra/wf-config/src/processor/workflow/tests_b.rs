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
        input_strict: None,
        triggered_subworkflow_config: None,
        metadata: None,
        created_at: 0,
        updated_at: 0,
        available_tools: None,
        hooks: None,
        layout: None,
    }
}

fn make_triggered_nodes() -> Vec<BaseStaticNode> {
    vec![
        BaseStaticNode {
            id: "msg-start".to_string(),
            node_type: StaticNodeType::StartFromMessage,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        },
        BaseStaticNode {
            id: "msg-end".to_string(),
            node_type: StaticNodeType::ContinueFromMessage,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        },
    ]
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
        error_route: None,
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
        error_route: None,
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
            error_route: None,
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
            error_route: None,
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
            error_route: None,
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
            error_route: None,
        },
    ];
    assert!(validate_workflow_definition(&wf).is_ok());
}

#[test]
fn test_triggered_subworkflow_type_requires_config() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
    wf.nodes = make_triggered_nodes();
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
    wf.nodes = make_triggered_nodes();
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
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
fn test_triggered_config_without_type_rejected() {
    let mut wf = make_workflow();
    wf.r#type = None;
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
        });
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("expected TriggeredSubworkflow"));
}

#[test]
fn test_triggered_config_with_standalone_type_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::Standalone);
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
        });
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("Standalone"));
    assert!(err.to_string().contains("expected TriggeredSubworkflow"));
}

#[test]
fn test_triggered_config_with_dependent_type_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::Dependent);
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
        });
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("Dependent"));
    assert!(err.to_string().contains("expected TriggeredSubworkflow"));
}

#[test]
fn test_dependent_plain_child_accepted() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::Dependent);
    wf.triggered_subworkflow_config = None;
    assert!(validate_workflow_definition(&wf).is_ok());
}

#[test]
fn test_dependent_with_message_nodes_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::Dependent);
    wf.triggered_subworkflow_config = None;
    wf.nodes = vec![BaseStaticNode {
        id: "node-1".to_string(),
        node_type: StaticNodeType::StartFromMessage,
        name: None,
        description: None,
        config: None,
        execution_config: None,
    }];
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("Dependent"));
    assert!(err.to_string().contains("TriggeredSubworkflow"));
}

#[test]
fn test_triggered_without_message_nodes_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
        });
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("TriggeredSubworkflow"));
    assert!(err.to_string().contains("message-channel"));
}

#[test]
fn test_triggered_with_plain_boundary_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
    wf.triggered_subworkflow_config =
        Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
            enable_checkpoints: None,
            timeout: None,
            compression_fallback: None,
        });
    wf.nodes = vec![
        BaseStaticNode {
            id: "start".to_string(),
            node_type: StaticNodeType::Start,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        },
        BaseStaticNode {
            id: "msg-start".to_string(),
            node_type: StaticNodeType::StartFromMessage,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        },
        BaseStaticNode {
            id: "msg-end".to_string(),
            node_type: StaticNodeType::ContinueFromMessage,
            name: None,
            description: None,
            config: None,
            execution_config: None,
        },
    ];
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("TriggeredSubworkflow"));
    assert!(err.to_string().contains("START/END"));
}

#[test]
fn test_standalone_with_message_nodes_rejected() {
    let mut wf = make_workflow();
    wf.r#type = Some(WorkflowDefinitionType::Standalone);
    wf.triggered_subworkflow_config = None;
    wf.nodes = make_triggered_nodes();
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("Standalone"));
    assert!(err.to_string().contains("TriggeredSubworkflow"));
}

#[test]
fn test_missing_type_with_message_nodes_rejected() {
    let mut wf = make_workflow();
    wf.r#type = None;
    wf.triggered_subworkflow_config = None;
    wf.nodes = make_triggered_nodes();
    let err = validate_workflow_definition(&wf).unwrap_err();
    assert!(err.to_string().contains("missing"));
    assert!(err.to_string().contains("TriggeredSubworkflow"));
}

#[test]
fn test_transform_error_edge_carries_route() {
    let configs = vec![WorkflowEdgeConfig {
        id: Some("e-err".to_string()),
        source_node_id: Some("n1".to_string()),
        target_node_id: Some("n2".to_string()),
        edge_type: Some("ERROR".to_string()),
        condition: None,
        label: None,
        description: None,
        weight: None,
        error_route: None,
    }];
    let edges = transform_edges(&configs).expect("error edge transforms");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].r#type, EdgeType::Error);
    assert_eq!(edges[0].id, "e-err".to_string());
}

#[test]
fn test_transform_error_edge_rejects_condition() {
    let configs = vec![WorkflowEdgeConfig {
        id: None,
        source_node_id: Some("n1".to_string()),
        target_node_id: Some("n2".to_string()),
        edge_type: Some("error".to_string()),
        condition: Some("${x}".to_string()),
        label: None,
        description: None,
        weight: None,
        error_route: None,
    }];
    let err = transform_edges(&configs).unwrap_err();
    assert!(err.to_string().contains("must not carry conditions"));
}

#[test]
fn test_transform_non_error_edge_rejects_error_route() {
    let configs = vec![WorkflowEdgeConfig {
        id: None,
        source_node_id: Some("n1".to_string()),
        target_node_id: Some("n2".to_string()),
        edge_type: None,
        condition: None,
        label: None,
        description: None,
        weight: None,
        error_route: Some(
            serde_json::from_value::<ErrorRouteConfig>(serde_json::json!({})).expect("empty route"),
        ),
    }];
    let err = transform_edges(&configs).unwrap_err();
    assert!(err.to_string().contains("not declared ERROR"));
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
        error_default: None,
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
        error_default: None,
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
        error_default: None,
    });
    assert!(validate_workflow_definition(&wf).is_ok());
}
