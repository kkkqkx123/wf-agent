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
        wf.hooks = Some(vec![wf_types::hook::HookPointConfig {
            hook_type: "BEFORE_ECECUTE".to_string(),
            condition: None,
            event_payload: None,
            enabled: Some(true),
            priority: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
        }]);
        assert!(validate_workflow_definition(&wf).is_err());
    }

    #[test]
    fn test_known_hook_type_accepted() {
        let mut wf = make_workflow();
        wf.hooks = Some(vec![wf_types::hook::HookPointConfig {
            hook_type: "WORKFLOW_BEFORE".to_string(),
            condition: None,
            event_payload: None,
            enabled: Some(true),
            priority: None,
            create_checkpoint: None,
            checkpoint_description: None,
            handler: None,
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
                edge_type: None,
                condition: None,
                label: None,
                description: None,
                weight: None,
                error_route: None,
            },
            WorkflowEdgeConfig {
                id: Some("e2".to_string()),
                source_node_id: Some("n2".to_string()),
                target_node_id: Some("n3".to_string()),
                edge_type: None,
                condition: Some("success".to_string()),
                label: None,
                description: None,
                weight: Some(1),
                error_route: None,
            },
        ];

        let edges = transform_edges(&configs).expect("valid edges transform");
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].r#type, EdgeType::Default);
        assert_eq!(edges[1].r#type, EdgeType::Conditional);
        assert_eq!(edges[1].id, "e2".to_string());
    }

    #[test]
    fn test_transform_nodes_keeps_plugin_types() {
        let configs = vec![WorkflowNodeConfig {
            id: "n1".to_string(),
            node_type: "LLMM".to_string(),
            name: None,
            description: None,
            config: None,
        }];
        let nodes = transform_nodes(&configs).expect("plugin types are kept");
        assert_eq!(nodes[0].node_type, StaticNodeType::Custom("LLMM".into()));
    }

    #[test]
    fn test_transform_nodes_rejects_empty_type() {
        let configs = vec![WorkflowNodeConfig {
            id: "n1".to_string(),
            node_type: "  ".to_string(),
            name: None,
            description: None,
            config: None,
        }];
        let err = transform_nodes(&configs).unwrap_err();
        assert!(err.to_string().contains("node_type"));
    }

    #[test]
    fn test_transform_edges_rejects_missing_endpoints() {
        let configs = vec![WorkflowEdgeConfig {
            id: None,
            source_node_id: Some("n1".to_string()),
            target_node_id: None,
            edge_type: None,
            condition: None,
            label: None,
            description: None,
            weight: None,
            error_route: None,
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
            error_default: None,
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
            error_default: None,
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
            error_default: None,
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
            error_default: None,
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
            error_default: None,
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
            error_default: None,
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
            error_default: None,
        });
        assert!(validate_workflow_definition(&wf).is_ok());
    }

    #[test]
    fn test_triggered_subworkflow_config_validation() {
        let mut wf = make_workflow();
        wf.r#type = Some(WorkflowDefinitionType::TriggeredSubworkflow);
        wf.nodes = make_triggered_nodes();
        wf.triggered_subworkflow_config =
            Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
                enable_checkpoints: None,
                timeout: Some(0),
                compression_fallback: None,
            });
        assert!(validate_workflow_definition(&wf).is_err());

        wf.triggered_subworkflow_config =
            Some(wf_types::workflow::definition::TriggeredSubworkflowConfig {
                enable_checkpoints: None,
                timeout: Some(5000),
                compression_fallback: None,
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
            error_route: None,
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
            error_route: None,
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
            error_route: None,
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
            error_route: None,
        }];
        // Edge existence is owned by GraphValidator; config layer only checks shape.
        assert!(validate_workflow_definition(&wf).is_ok());
    }
