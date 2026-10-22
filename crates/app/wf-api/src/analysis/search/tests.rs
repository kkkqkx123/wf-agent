    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use crate::ApiContext;
    use wf_storage::adapter::base::BaseStorageAdapter;
    use wf_resource::registry::{RegisterOptions as ResourceOptions, ResourceRegistries};
    use wf_storage::context::StorageContext;
    use wf_types::events::{BaseEvent, EventType};
    use wf_types::workflow::{WorkflowDefinition, WorkflowMetadata, WorkflowTemplate};
    use wf_types::ExecutionStatus;

    fn make_ctx() -> (Arc<ResourceRegistries>, Arc<ApiContext>) {
        let storage = StorageContext::new_memory();
        let registries = Arc::new(ResourceRegistries::new());
        wf_resource::register_all(
            &registries,
            &Arc::new(wf_tools::registry::ToolRegistry::new()),
            &ResourceOptions::default(),
        );
        let ctx = Arc::new(ApiContext::new(storage, registries.clone()));
        (registries, ctx)
    }

    fn ctx_only() -> Arc<ApiContext> {
        let (_, ctx) = make_ctx();
        ctx
    }

    fn make_workflow_template(id: &str, name: &str, tags: Option<Vec<String>>) -> WorkflowTemplate {
        WorkflowTemplate {
            id: id.to_string(),
            name: name.to_string(),
            description: "A test workflow".to_string(),
            definition: WorkflowDefinition {
                id: id.to_string(),
                name: name.to_string(),
                description: Some("desc-target".to_string()),
                r#type: None,
                version: None,
                nodes: vec![],
                edges: vec![],
                config: None,
                variables: None,
                triggered_subworkflow_config: None,
                metadata: Some(WorkflowMetadata {
                    author: None,
                    tags,
                    category: None,
                    ..Default::default()
                }),
                available_tools: None,
                hooks: None,
                layout: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            },
            template_category: None,
            template_tags: Some(vec!["blueprint".to_string()]),
            is_public: None,
            is_system: None,
            enabled: None,
            is_published: None,
        }
    }

    fn make_execution(id: &str, workflow_id: &str) -> wf_types::WorkflowExecution {
        wf_types::WorkflowExecution {
            id: id.into(),
            workflow_id: workflow_id.into(),
            workflow_version: None,
            status: ExecutionStatus::Completed,
            current_node_id: None,
            graph: None,
            variables: None,
            input: None,
            output: None,
            node_results: None,
            errors: None,
            error: None,
            started_at: wf_common::now(),
            completed_at: None,
            execution_type: None,
            fork_join_context: None,
            hierarchy: None,
        }
    }

    #[test]
    fn test_score() {
        assert_eq!(score("abc", &["abc".into(), "zzz".into()]), 2);
        assert_eq!(score("zzz", &["abc".into(), "xzzzy".into()]), 1);
        assert_eq!(score("none", &["abc".into(), "zzz".into()]), 0);
    }

    #[tokio::test]
    async fn test_search_finds_workflows_and_executions() {
        use wf_core::registry::MutableRegistry;

        let (registries, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-target".to_string(),
                Arc::new(make_workflow_template("wf-target", "target-flow", None)),
            )
            .expect("register workflow");

        ctx.storage
            .workflow_execution
            .save(&make_execution("exec-target-1", "wf-1"))
            .await
            .unwrap();

        let result = search(&ctx, "target", &SearchOptions::default())
            .await
            .unwrap();

        assert!(result.items.iter().any(|i| i.r#type == "execution"));
        assert!(result.by_type.contains_key("execution"));
        assert!(result.items.iter().any(|i| i.r#type == "workflow"));
        assert_eq!(result.query, "target");
    }

    #[tokio::test]
    async fn test_search_filters_by_type() {
        use wf_core::registry::MutableRegistry;

        let (registries, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-target".to_string(),
                Arc::new(make_workflow_template("wf-target", "target-flow", None)),
            )
            .expect("register workflow");
        ctx.storage
            .workflow_execution
            .save(&make_execution("exec-target-1", "wf-1"))
            .await
            .unwrap();

        let result = search(
            &ctx,
            "target",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Execution]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();

        assert!(result.items.iter().all(|i| i.r#type == "execution"));
        assert_eq!(result.items.len(), 1);
        assert!(!result.by_type.contains_key("workflow"));
    }

    #[tokio::test]
    async fn test_search_matches_description_and_tags() {
        use wf_core::registry::MutableRegistry;

        let (registries, ctx) = make_ctx();
        registries
            .workflows
            .register(
                "wf-tagged".to_string(),
                Arc::new(make_workflow_template(
                    "wf-tagged",
                    "flow-a",
                    Some(vec!["salesforce".to_string()]),
                )),
            )
            .expect("register workflow");

        let by_description = search(
            &ctx,
            "desc-target",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Workflow]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_description.items.len(), 1);

        let by_tag = search(
            &ctx,
            "salesforce",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Workflow]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(by_tag.items.len(), 1);
    }

    #[tokio::test]
    async fn test_search_events_from_event_bus() {
        let ctx = ctx_only();
        let _sub = ctx.event_bus.subscribe();
        let event = BaseEvent {
            id: wf_common::generate_id(),
            r#type: EventType::WorkflowExecutionStarted,
            timestamp: wf_common::now(),
            workflow_id: Some("wf-target".to_string()),
            execution_id: Some("exec-target".to_string()),
            agent_loop_id: None,

            event_name: None,
            metadata: None,
        };
        ctx.event_bus.publish(event).unwrap();

        let result = search(
            &ctx,
            "workflow_execution_started",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Event]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].r#type, "event");
    }

    #[tokio::test]
    async fn test_search_limit_and_truncation() {
        let ctx = ctx_only();
        for i in 0..30 {
            let task = wf_types::TaskStorageMetadata {
                id: format!("task-match-{:02}", i),
                task_type: "search".into(),
                status: "pending".into(),
                execution_id: None,
                instance_id: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            };
            ctx.storage.task.save(&task).await.unwrap();
        }

        let result = search(
            &ctx,
            "match",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Task]),
                limit_per_type: Some(5),
                limit_total: Some(3),
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(result.total, 3);
        assert!(result.truncated);
    }

    #[tokio::test]
    async fn test_search_results_are_deterministic() {
        use wf_core::registry::MutableRegistry;

        let (registries, ctx) = make_ctx();
        for i in 0..10 {
            registries
                .workflows
                .register(
                    format!("wf-{i:02}"),
                    Arc::new(make_workflow_template(
                        &format!("wf-{i:02}"),
                        &format!("flow-{i:02}"),
                        None,
                    )),
                )
                .expect("register workflow");
        }

        let options = SearchOptions {
            types: Some(vec![SearchResourceType::Workflow]),
            limit_per_type: Some(5),
            ..SearchOptions::default()
        };
        let first = search(&ctx, "flow", &options).await.unwrap();
        let second = search(&ctx, "flow", &options).await.unwrap();
        let first_ids: Vec<String> = first.items.iter().map(|i| i.id.clone()).collect();
        let second_ids: Vec<String> = second.items.iter().map(|i| i.id.clone()).collect();
        assert_eq!(first_ids, second_ids);
    }

    #[tokio::test]
    async fn test_search_message_domain_carries_loop_ownership() {
        use wf_types::message::{Message, MessageContentValue, MessageRole};

        let ctx = ctx_only();
        for i in 0..3 {
            crate::entity::message::add_message(
                &ctx,
                "exec-msg",
                Some("loop-msg"),
                Message {
                    id: format!("msg-{i}"),
                    role: MessageRole::User,
                    content: MessageContentValue::Text(format!("deploy token {i}")),
                    timestamp: 100 + i,
                    tool_call_id: None,
                    tool_name: None,
                    tool_calls: None,
                    thinking: None,
                    thinking_signature: None,
                    metadata: None,
                },
            )
            .await
            .unwrap();
        }
        let result = search(
            &ctx,
            "deploy",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Message]),
                ..SearchOptions::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(result.items.len(), 3);
        assert!(result.items.iter().all(|i| i.r#type == "message"));
        assert!(result
            .items
            .iter()
            .all(|i| i.agent_loop_id.as_deref() == Some("loop-msg")));
        assert!(result
            .items
            .iter()
            .all(|i| i.execution_id.as_deref() == Some("exec-msg")));
    }

    #[tokio::test]
    async fn test_search_cursor_continues_across_pages() {
        let ctx = ctx_only();
        for i in 0..30 {
            let task = wf_types::TaskStorageMetadata {
                id: format!("task-cursor-{:02}", i),
                task_type: "search".into(),
                status: "pending".into(),
                execution_id: None,
                instance_id: None,
                created_at: wf_common::now(),
                updated_at: wf_common::now(),
            };
            ctx.storage.task.save(&task).await.unwrap();
        }
        let first = search(
            &ctx,
            "cursor",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Task]),
                limit_per_type: Some(30),
                limit_total: Some(10),
                cursor: None,
            },
        )
        .await
        .unwrap();
        assert!(first.truncated);
        let cursor = first.next_cursor.clone().expect("cursor present");
        let second = search(
            &ctx,
            "cursor",
            &SearchOptions {
                types: Some(vec![SearchResourceType::Task]),
                limit_per_type: Some(30),
                limit_total: Some(10),
                cursor: Some(cursor),
            },
        )
        .await
        .unwrap();
        assert_eq!(second.total, 10);
        let first_ids: std::collections::HashSet<&str> =
            first.items.iter().map(|i| i.id.as_str()).collect();
        for item in &second.items {
            assert!(!first_ids.contains(item.id.as_str()), "no repeats");
        }
    }

    #[tokio::test]
    async fn test_cursor_malformed_degrades_to_start() {
        assert!(decode_cursor(None).is_empty());
        assert!(decode_cursor(Some("zz")).is_empty());
        let mut offsets = BTreeMap::new();
        offsets.insert("task".to_string(), 7);
        let token = encode_cursor(&offsets);
        assert_eq!(decode_cursor(Some(&token)).get("task"), Some(&7));
    }
