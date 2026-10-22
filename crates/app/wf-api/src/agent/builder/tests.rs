    use super::*;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use crate::ApiContext;
    use wf_types::agent::AgentHookType;
    use wf_types::llm::tool_call_protocol::ToolCallProtocolConfig;

    #[test]
    fn tool_config_requires_at_least_one_tool() {
        let config = AgentToolConfigBuilder::new()
            .add_tool("web_search")
            .with_initial(vec!["web_search"])
            .require_approval(vec!["shell_exec"])
            .build();
        assert_eq!(config.available, vec!["web_search".to_string()]);
        assert_eq!(config.initial, Some(vec!["web_search".to_string()]));
        assert_eq!(
            config.require_approval,
            Some(vec!["shell_exec".to_string()])
        );
    }

    #[test]
    fn hook_builder_produces_typed_hook() {
        let hook = AgentHookBuilder::before_tool_call()
            .condition("${tool_name} == 'shell'")
            .create_checkpoint()
            .build();
        assert_eq!(hook.hook_type, AgentHookType::BeforeToolCall);
        assert_eq!(hook.create_checkpoint, Some(true));
    }

    #[test]
    fn add_hook_preserves_priority_and_payload() {
        let hook = AgentHookBuilder::before_tool_call()
            .priority(7)
            .event_payload(serde_json::json!({"k": "v"}))
            .build();
        let config = AgentLoopConfigBuilder::new("a")
            .model("mock")
            .add_hook(hook)
            .build();
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].hook_type, "BEFORE_TOOL_CALL");
        assert_eq!(config.hooks[0].priority, 7);
        assert_eq!(config.hooks[0].payload, Some(serde_json::json!({"k": "v"})));
    }

    #[test]
    fn add_hook_preserves_all_five_converted_fields() {
        let hook = AgentHookBuilder::after_tool_call()
            .condition("flag")
            .enabled(false)
            .priority(3)
            .event_payload(serde_json::json!({"k": "v"}))
            .handler("audit-handler")
            .build();
        let config = AgentLoopConfigBuilder::new("a")
            .model("mock")
            .add_hook(hook)
            .build();
        assert_eq!(config.hooks.len(), 1);
        let converted = &config.hooks[0];
        assert_eq!(converted.hook_type, "AFTER_TOOL_CALL");
        assert_eq!(converted.condition.as_deref(), Some("flag"));
        assert!(!converted.enabled);
        assert_eq!(converted.priority, 3);
        assert_eq!(converted.payload, Some(serde_json::json!({"k": "v"})));
        assert_eq!(converted.handler.as_deref(), Some("audit-handler"));
    }

    #[test]
    fn agent_definition_builder_validates_and_builds() {
        let definition = AgentDefinitionBuilder::new("agent-1")
            .name("Code Agent")
            .version("1.0.0")
            .build()
            .expect("named agent must build");
        assert_eq!(definition.name, "Code Agent");
        assert_eq!(definition.id, "agent-1");
    }

    #[test]
    fn agent_definition_builder_rejects_empty_name() {
        let err = AgentDefinitionBuilder::new("agent-2")
            .name("")
            .build()
            .unwrap_err();
        assert!(matches!(err, crate::ApiError::Validation(_)));
    }

    #[test]
    fn agent_definition_builder_config_setters() {
        let definition = AgentDefinitionBuilder::new("agent-cfg")
            .name("Configured Agent")
            .system_prompt("You are a code agent.")
            .max_iterations(8)
            .available_tools(AgentToolConfigBuilder::new().add_tool("web_search").build())
            .add_hook(AgentHookBuilder::after_iteration().build())
            .build()
            .expect("named agent must build");

        let config = definition.config.expect("config built");
        assert_eq!(
            config.system_prompt.as_deref(),
            Some("You are a code agent.")
        );
        assert_eq!(config.max_iterations, Some(8));
        assert_eq!(
            config.available_tools.as_ref().unwrap().available,
            vec!["web_search".to_string()]
        );
        assert_eq!(config.hooks.as_ref().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn agent_definition_register_adds_template() {
        let ctx = Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(ResourceRegistries::new()),
        ));
        AgentDefinitionBuilder::new("agent-reg")
            .name("Registered Agent")
            .register(&ctx)
            .await
            .expect("register must succeed");
        assert!(ctx.registries.agent_templates.has("agent-reg"));
    }

    #[test]
    fn loop_config_builder_requires_model() {
        let config = AgentLoopConfigBuilder::new("agent-1")
            .model("mock")
            .max_iterations(5)
            .add_tool("web_search")
            .add_hook(AgentHookBuilder::after_iteration().enabled(false).build())
            .build();
        assert_eq!(config.model, "mock");
        assert_eq!(config.max_iterations, Some(5));
        assert_eq!(
            config.available_tool_names,
            Some(vec!["web_search".to_string()])
        );
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].hook_type, "AFTER_ITERATION");
        assert!(!config.hooks[0].enabled);
    }

    #[test]
    fn loop_config_builder_carries_tool_call_protocol_config() {
        let config = AgentLoopConfigBuilder::new("agent-1")
            .model("mock")
            .tool_call_protocol(ToolCallProtocolConfig {
                format: wf_types::llm::ToolCallProtocol::Xml,
                markers: None,
                xml_tags: None,
                include_description: Some(false),
                description_style: Some("detailed".to_string()),
                include_examples: None,
                include_rules: None,
                additional_config: None,
            })
            .build();
        let format = config.tool_call_protocol.expect("config must flow through");
        assert_eq!(format.format, wf_types::llm::ToolCallProtocol::Xml);
        assert_eq!(format.include_description, Some(false));
        assert_eq!(format.description_style.as_deref(), Some("detailed"));
    }

