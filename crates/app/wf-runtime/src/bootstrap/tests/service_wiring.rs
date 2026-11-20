use super::*;

#[test]
fn test_adjust_log_config_json_mode() {
    use crate::logger::LogFormat;

    let mode = ModeInfo {
        mode: ExecutionMode::Headless,
        output_format: crate::mode::OutputFormat::Json,
        color_enabled: false,
    };

    let config = LogConfig::default();
    let adjusted = adjust_log_config(config, &mode);

    assert_eq!(adjusted.format, Some(LogFormat::Json));
}

#[test]
fn test_adjust_log_config_silent_mode() {
    let mode = ModeInfo {
        mode: ExecutionMode::Interactive,
        output_format: crate::mode::OutputFormat::Silent,
        color_enabled: false,
    };

    let config = LogConfig::default().with_level(crate::logger::LogLevel::Info);
    let adjusted = adjust_log_config(config, &mode);

    assert_eq!(adjusted.level, Some(crate::logger::LogLevel::Off));
}

#[test]
fn test_resolve_log_config_respects_explicit_level() {
    use crate::logger::LogLevel;
    use wf_config::file_layer::FileLayerConfig;

    clear_env_vars();
    let layer = FileLayerConfig {
        log_level: Some("debug".to_string()),
        ..Default::default()
    };
    let mode = super::test_mode_info();

    let from_file = resolve_log_config(RuntimeConfig::default(), Some(&layer), &mode).unwrap();
    assert_eq!(from_file.log_config.level, Some(LogLevel::Debug));

    let explicit = RuntimeConfig {
        log_config: LogConfig::default().with_level(LogLevel::Error),
        ..Default::default()
    };
    let resolved = resolve_log_config(explicit, Some(&layer), &mode).unwrap();
    assert_eq!(resolved.log_config.level, Some(LogLevel::Error));
}

#[test]
fn test_resolve_log_config_rejects_invalid_file_level() {
    use wf_config::file_layer::FileLayerConfig;

    clear_env_vars();
    let layer = FileLayerConfig {
        log_level: Some("verbose".to_string()),
        ..Default::default()
    };

    let result = resolve_log_config(
        RuntimeConfig::default(),
        Some(&layer),
        &super::test_mode_info(),
    );
    assert!(result.is_err());
}

#[test]
fn test_select_log_override_treats_empty_env_as_unset() {
    assert_eq!(
        select_log_override(Some(String::new()), Some("debug".to_string())),
        Some("debug".to_string())
    );
    assert_eq!(
        select_log_override(Some("warn".to_string()), Some("debug".to_string())),
        Some("warn".to_string())
    );
    assert_eq!(select_log_override(None, None), None);
}

#[test]
fn test_resolve_log_config_resolves_format() {
    use crate::logger::LogFormat;
    use wf_config::file_layer::FileLayerConfig;

    clear_env_vars();
    let layer = FileLayerConfig {
        log_format: Some("json".to_string()),
        ..Default::default()
    };
    let mode = super::test_mode_info();

    let from_file = resolve_log_config(RuntimeConfig::default(), Some(&layer), &mode).unwrap();
    assert_eq!(from_file.log_config.format, Some(LogFormat::Json));

    let explicit = RuntimeConfig {
        log_config: LogConfig::default().with_format(LogFormat::Compact),
        ..Default::default()
    };
    let resolved = resolve_log_config(explicit, Some(&layer), &mode).unwrap();
    assert_eq!(resolved.log_config.format, Some(LogFormat::Compact));
}

#[test]
fn test_resolve_log_config_rejects_invalid_file_format() {
    use wf_config::file_layer::FileLayerConfig;

    clear_env_vars();
    let layer = FileLayerConfig {
        log_format: Some("verbose".to_string()),
        ..Default::default()
    };

    let result = resolve_log_config(
        RuntimeConfig::default(),
        Some(&layer),
        &super::test_mode_info(),
    );
    assert!(result.is_err());
}

#[tokio::test]
async fn test_bootstrap_registers_mcp_manager_and_use_mcp() {
    clear_env_vars();

    let root = std::env::temp_dir().join(format!("wf-runtime-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // A lazy server: registered but not connected, so no process spawn.
    std::fs::write(
        root.join("mcp-settings.json"),
        r#"{"mcpServers": {"echo-srv": {"type": "stdio", "command": "echo", "timeout": 5}}}"#,
    )
    .unwrap();

    let config = RuntimeConfig {
        mcp: McpRuntimeConfig {
            settings_dir: Some(root.clone()),
            project_root: Some(root.clone()),
        },
        ..default_test_config()
    };

    let runtime = Runtime::bootstrap(config).await.unwrap();

    let manager = runtime.mcp_manager().expect("MCP manager initialized");
    assert_eq!(manager.registry().list().len(), 1);
    assert!(
        manager.connected_servers().is_empty(),
        "lazy server not connected"
    );

    // use_mcp is registered into the shared tool registry.
    assert!(runtime.tool_registry().get_tool("use_mcp").is_some());

    runtime.shutdown().await.unwrap();
    let _ = std::fs::remove_dir_all(&root);
    clear_env_vars();
}

#[test]
fn test_llm_gateway_registers_and_rejects_invalid_profiles() {
    let profiles = vec![wf_types::llm::LlmProfile {
        id: "openai".to_string(),
        name: "OpenAI".to_string(),
        format: wf_types::llm::LlmFormat::OpenaiChat,
        provider_id: None,
        model: "gpt-4o".to_string(),
        api_key: None,
        base_url: None,
        parameters: None,
        generation: None,
        timeout: None,
        max_retries: None,
        retry_delay: None,
        headers: None,
        metadata: None,
        tool_call_protocol: None,
        auth_type: None,
        custom_headers: None,
        custom_body: None,
        custom_body_enabled: None,
        query_params: None,
        stream_options: None,
        context_window_size: None,
        max_output_tokens: None,
        proxy: None,
        no_proxy: None,
        circuit_breaker: None,
        prompt_caching: None,
        auth: None,
        compat: None,
        identity: None,
        transport: None,
        env: None,
    }];
    let gateway = init_llm_gateway(
        &LlmConfig {
            profiles,
            provider_definitions: Vec::new(),
            auth: Default::default(),
            catalog_refresh: None,
        },
        None,
        None,
        None,
    )
    .unwrap();
    assert!(gateway.has_profile("openai"));

    let err = match init_llm_gateway(
        &LlmConfig {
            profiles: vec![wf_types::llm::LlmProfile {
                id: String::new(),
                name: "broken".to_string(),
                format: wf_types::llm::LlmFormat::OpenaiChat,
                provider_id: None,
                model: String::new(),
                api_key: None,
                base_url: None,
                parameters: None,
                generation: None,
                timeout: None,
                max_retries: None,
                retry_delay: None,
                headers: None,
                metadata: None,
                tool_call_protocol: None,
                auth_type: None,
                custom_headers: None,
                custom_body: None,
                custom_body_enabled: None,
                query_params: None,
                stream_options: None,
                context_window_size: None,
                max_output_tokens: None,
                proxy: None,
                no_proxy: None,
                circuit_breaker: None,
                prompt_caching: None,
                auth: None,
                compat: None,
                identity: None,
                transport: None,
                env: None,
            }],
            provider_definitions: Vec::new(),
            auth: Default::default(),
            catalog_refresh: None,
        },
        None,
        None,
        None,
    ) {
        Ok(_) => panic!("invalid profile must be rejected"),
        Err(e) => e,
    };
    assert!(err.to_string().contains("LLM profile"));
}

#[tokio::test]
async fn test_shell_events_bridged_to_event_bus() {
    clear_env_vars();

    let config = RuntimeConfig {
        shell: wf_shell::config::ShellToolConfig {
            output_event_enabled: true,
            ..Default::default()
        },
        ..default_test_config()
    };

    let runtime = Runtime::bootstrap(config).await.unwrap();
    let mut sub = runtime.event_bus.subscribe();

    // Tool definitions are registered by wf-resource in production;
    // register the shell defs used by this test directly.
    runtime
        .tool_registry()
        .register_tool(wf_tools::predefined::shell::GET_OR_CREATE_SHELL.tool_def());

    std::fs::create_dir_all("/tmp/bootstrap-shell-events").unwrap();
    let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("exec-bridge".into());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };
    let result = runtime
        .tool_registry()
        .execute_tool(
            "get_or_create_shell",
            &serde_json::json!({ "cwd": "/tmp/bootstrap-shell-events" }),
            &options,
            &ctx,
        )
        .await
        .unwrap();
    assert!(result.success);

    // The created event is delivered on a background dispatch thread;
    // poll until it arrives.
    let expected_session = result.result.unwrap()["session_id"].clone();
    let mut created_event: Option<wf_types::events::BaseEvent> = None;
    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(20),
            std::time::Duration::from_secs(8),
            || {
                let matched = match sub.try_recv() {
                    Ok(event)
                        if event.r#type == wf_types::events::EventType::ShellSessionCreated =>
                    {
                        created_event = Some(event);
                        true
                    }
                    _ => false,
                };
                async move { matched }
            },
        )
        .await,
        "no ShellSessionCreated event on the runtime EventBus"
    );
    assert_eq!(
        created_event.unwrap().metadata.unwrap()["session_id"],
        expected_session
    );

    runtime.shutdown().await.unwrap();
    clear_env_vars();
}
