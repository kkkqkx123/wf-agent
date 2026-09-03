#[allow(clippy::module_inception)]
#[cfg(test)]
mod tests {
    use crate::bootstrap::{
        adjust_log_config, init_checkpoint_store, init_llm_gateway, resolve_infra_config,
        storage_db_path, InfraSourceConfig, LlmConfig, McpRuntimeConfig, ResourceConfig, Runtime,
        RuntimeConfig,
    };
    #[cfg(feature = "plugins")]
    use crate::bootstrap::PluginConfig;
    use crate::logger::LogConfig;
    use crate::mode::{ExecutionMode, ModeInfo};
    use std::path::PathBuf;
    use std::sync::Arc;
    use wf_core::registry::Registry;
    use wf_types::config::metrics::MetricsConfig;
    use wf_types::config::storage::{StorageConfig, StorageType};
    use wf_types::config::timeout::TimeoutConfig;

    fn clear_env_vars() {
        std::env::remove_var("CLI_MODE");
        std::env::remove_var("HEADLESS");
        std::env::remove_var("TEST_MODE");
        std::env::remove_var("CLI_OUTPUT_FORMAT");
        std::env::remove_var("NO_COLOR");
    }

    fn memory_storage_config() -> StorageConfig {
        StorageConfig {
            storage_type: StorageType::Memory,
            ..Default::default()
        }
    }

    fn default_test_config() -> RuntimeConfig {
        RuntimeConfig {
            storage: memory_storage_config(),
            log_config: LogConfig::default().with_level("off"),
            mode_override: Some(ExecutionMode::Test),
            resource: ResourceConfig::default(),
            metrics: None,
            llm: LlmConfig::default(),
            sandbox: None,
            skills: Default::default(),
            mcp: Default::default(),
            shell: Default::default(),
            #[cfg(feature = "plugins")]
            plugins: PluginConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn default_tool_context() -> (
        wf_tools::executor::trait_def::ToolExecutionContext,
        wf_types::tool::ToolExecutionOptions,
    ) {
        let ctx =
            wf_tools::executor::trait_def::ToolExecutionContext::new("callback-test".into());
        let options = wf_types::tool::ToolExecutionOptions {
            timeout: None,
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        };
        (ctx, options)
    }

    fn register_agent_tools(runtime: &Runtime) {
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::agent::CALL_AGENT.tool_def());
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::QUERY_WORKFLOW_STATUS.tool_def());
    }

    fn register_workflow_tools(runtime: &Runtime) {
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::EXECUTE_WORKFLOW.tool_def());
        runtime
            .tool_registry()
            .register_tool(wf_tools::predefined::workflow::QUERY_WORKFLOW_STATUS.tool_def());
    }

    fn mock_llm_gateway(runtime: &Runtime, profile: &str, text: &str) {
        let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
        mock.default(wf_llm::mock::LlmResponseSpec::text(text));
        runtime.llm_gateway().register_mock(profile, mock);
    }

    /// Query an execution through the query_workflow_status tool, exercising
    /// the registry-bound composite callback exactly like production.
    async fn query_status_via_tool(runtime: &Runtime, execution_id: &str) -> serde_json::Value {
        let (ctx, options) = default_tool_context();
        let result = runtime
            .tool_registry()
            .execute_tool(
                "query_workflow_status",
                &serde_json::json!({ "workflow_id": execution_id, "execution_id": execution_id }),
                &options,
                &ctx,
            )
            .await
            .expect("query tool must succeed");
        assert!(result.success, "query failed: {:?}", result.error);
        result.result.unwrap()
    }

    async fn poll_until_completed(runtime: &Runtime, execution_id: &str) -> serde_json::Value {
        for _ in 0..200 {
            let status = query_status_via_tool(runtime, execution_id).await;
            if status["status"] == "completed" {
                return status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("execution {execution_id} did not complete within poll window");
    }

    /// The checked-in `configs/infrastructure/` bundle (development preset)
    /// fills the runtime config where programmatic values are absent, while
    /// programmatic values keep priority.
    #[test]
    fn test_resolve_infra_config_from_repo_configs() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        if !repo_root.join("configs").join("infrastructure").exists() {
            return;
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let config = runtime.block_on(resolve_infra_config(
            RuntimeConfig::default(),
            &InfraSourceConfig {
                project_root: Some(repo_root.clone()),
                ..Default::default()
            },
        ));
        let config = config.unwrap();

        // File layer fills storage/metrics/timeout/output/sandbox/presets/
        // tools/file_checkpoint from the development preset.
        assert_ne!(config.storage, StorageConfig::default());
        assert!(config.metrics.is_some());
        assert_eq!(config.timeout.default, Some(30000));
        assert_eq!(config.output.dir, "./outputs");
        assert!(config.sandbox.is_some());
        assert!(config.tools.read_file.is_some());
        assert!(
            config.file_checkpoint.custom_ignore_patterns.is_some(),
            "file_checkpoint must load from the file layer"
        );

        // MCP/skills sources are inherited.
        assert_eq!(config.mcp.project_root, Some(repo_root.clone()));

        // Programmatic values win over the file layer.
        let programmatic = RuntimeConfig {
            metrics: Some(MetricsConfig::default()),
            timeout: TimeoutConfig {
                default: Some(11111),
                ..Default::default()
            },
            ..Default::default()
        };
        let config = runtime.block_on(resolve_infra_config(
            programmatic,
            &InfraSourceConfig {
                project_root: Some(repo_root),
                ..Default::default()
            },
        ));
        let config = config.unwrap();
        assert_eq!(config.timeout.default, Some(11111));
        assert!(config.metrics.is_some());
        // storage was left default -> still filled from the file layer.
        assert_ne!(config.storage, StorageConfig::default());
    }

    #[tokio::test]
    async fn test_runtime_bootstrap_memory() {
        clear_env_vars();

        let config = default_test_config();
        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(runtime.storage().is_initialized());
        assert!(runtime.mode().is_test());
        assert!(!runtime.tool_registry.list().is_empty());

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[test]
    fn test_storage_db_path_resolution() {
        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: None,
            postgres: None,
            app_name: Some("myapp".into()),
        };
        assert_eq!(
            storage_db_path(&config),
            PathBuf::from("./storage/myapp.db")
        );

        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: "/data/custom.db".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: Some("myapp".into()),
        };
        assert_eq!(storage_db_path(&config), PathBuf::from("/data/custom.db"));
    }

    #[tokio::test]
    async fn test_init_checkpoint_store_memory() {
        let config = memory_storage_config();
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Memory(_)
        ));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_init_checkpoint_store_sqlite_roundtrip() {
        use wf_storage::domain::Store;

        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: ":memory:".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        };
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Sqlite(_)
        ));

        let (data, meta) = (
            b"checkpoint-data".to_vec(),
            serde_json::json!({"entityType": "checkpoint"}),
        );
        store.save("cp-1", &data, &meta).await.unwrap();
        let loaded = store.load("cp-1").await.unwrap().unwrap();
        assert_eq!(loaded.0, data);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_init_checkpoint_store_sqlite_fallback_on_error() {
        let config = StorageConfig {
            storage_type: StorageType::Sqlite,
            sqlite: Some(wf_types::config::storage::SqliteStorageConfig {
                db_path: "/nonexistent-dir-xyz/foo.db".into(),
                ..Default::default()
            }),
            postgres: None,
            app_name: None,
        };
        let store = init_checkpoint_store(&config).await;
        assert!(matches!(
            *store,
            wf_storage::backend::StorageBackend::Memory(_)
        ));
    }

    #[tokio::test]
    async fn test_runtime_registries_populated() {
        clear_env_vars();

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig::default(),
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Verify registries are populated from register_all()
        assert!(!runtime.registries().fragments.is_empty());
        assert!(!runtime.registries().templates.is_empty());
        assert!(!runtime.registries().tool_descriptions.is_empty());
        assert!(!runtime.registries().agent_templates.is_empty());
        assert!(!runtime.registries().workflows.is_empty());

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_api_context_shared_and_cached() {
        clear_env_vars();

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig::default(),
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let first: &wf_api::ApiContext = runtime.api_context();
        let second: &wf_api::ApiContext = runtime.api_context();
        assert!(std::ptr::eq(first, second), "api_context must be cached");

        // The context shares the runtime's event bus and tool registry.
        assert!(Arc::ptr_eq(&first.event_bus, &runtime.event_bus));
        assert!(Arc::ptr_eq(&first.tool_registry, &runtime.tool_registry));

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[cfg(feature = "plugins")]
    #[tokio::test]
    async fn test_runtime_goal_review_resource_plugin_activation() {
        clear_env_vars();

        use wf_resource::registry::{RegisterOptions as ResourceOptions, ResourcePluginActivation};
        use wf_workflow::validation::GraphValidator;

        let config = RuntimeConfig {
            log_config: LogConfig::default().with_level("off"),
            resource: ResourceConfig {
                options: ResourceOptions {
                    resource_plugin_activation: vec![ResourcePluginActivation {
                        id: "@standard/goal-review-agent".into(),
                        config: serde_json::json!({
                            "root_requirement": "fix the failing test",
                            "max_iterations": 3,
                        }),
                    }],
                    ..Default::default()
                },
            },
            metrics: None,
            ..Default::default()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        // Built-in resource plugin registered and activated through the
        // unified plugin engine: workflow + planner prompt land in the
        // registries via the contribution bridge.
        let engine = runtime
            .plugin_engine()
            .expect("plugin engine is enabled by default");
        assert!(engine.registry().has("@standard/goal-review-agent"));
        assert_eq!(
            engine
                .registry()
                .get("@standard/goal-review-agent")
                .unwrap()
                .status,
            wf_plugin::PluginStatus::Active
        );

        assert!(runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(runtime
            .registries()
            .templates
            .has("@standard/goal-review-planner"));

        // The assembled workflow is structurally valid (loop pairs, edges,
        // reachability) so it can be executed by the workflow engine.
        let wf = runtime
            .registries()
            .workflows
            .get("@standard/goal-review-agent-workflow")
            .expect("goal review workflow registered");
        let graph = crate::trigger_listener::template_to_graph(&wf);
        GraphValidator::validate(graph).unwrap_or_else(|errors| {
            panic!(
                "goal review workflow failed validation: {:?}",
                errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
            )
        });

        // Deactivation via the plugin engine removes the workflow and prompt.
        engine
            .deactivate("@standard/goal-review-agent")
            .await
            .unwrap();
        assert_eq!(
            engine
                .registry()
                .get("@standard/goal-review-agent")
                .unwrap()
                .status,
            wf_plugin::PluginStatus::Deactivated
        );
        assert!(!runtime
            .registries()
            .workflows
            .has("@standard/goal-review-agent-workflow"));
        assert!(!runtime
            .registries()
            .templates
            .has("@standard/goal-review-planner"));

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_sandbox_config_valid() {
        clear_env_vars();

        use wf_types::script::sandbox::{
            SandboxConfig, SandboxGlobalConfig, SandboxMode, SandboxProfile, SandboxProfileRule,
            SandboxRuleMatchField,
        };

        // Valid global config: bootstrap succeeds and exposes the compiled
        // shared sandbox runtime; the rule routes shell executions to the
        // Lenient profile.
        let global = SandboxGlobalConfig {
            mode: Some(SandboxMode::Strict),
            profiles: vec![SandboxProfile {
                name: "lenient".to_string(),
                description: None,
                mode: Some(SandboxMode::Lenient),
                shell_strategy: None,
                python_strategy: None,
                javascript_strategy: None,
                lua_strategy: None,
                policy: None,
                vfs: None,
                workdir: None,
                env: None,
            }],
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "shell".to_string(),
                profile: "lenient".to_string(),
            }],
            default_profile: None,
            audit_logging: true,
        };
        let config = RuntimeConfig {
            sandbox: Some(global),
            ..Default::default()
        };
        let runtime = Runtime::bootstrap(config).await.unwrap();
        let result = runtime
            .sandbox_runtime()
            .execute(
                "shell",
                "echo hello",
                &SandboxConfig {
                    mode: None,
                    policy: None,
                    shell_strategy: None,
                    python_strategy: None,
                    javascript_strategy: None,
                    lua_strategy: None,
                    vfs: None,
                    workdir: None,
                    env: None,
                    legacy_type: None,
                    resource_limits: None,
                    skip_gate_check: None,
                },
            )
            .await;
        assert!(
            result.success,
            "shared sandbox runtime must execute shell: {:?}",
            result.error
        );
        assert_eq!(
            result.sandbox_mode,
            Some("Lenient".to_string()),
            "rule must route shell to the lenient profile"
        );
        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_sandbox_config_invalid_fails_fast() {
        clear_env_vars();

        use wf_types::script::sandbox::{
            SandboxGlobalConfig, SandboxProfileRule, SandboxRuleMatchField,
        };

        // Invalid config (rule references unknown profile): bootstrap must
        // fail fast instead of deferring the error to script execution.
        let bad = SandboxGlobalConfig {
            rules: vec![SandboxProfileRule {
                match_field: SandboxRuleMatchField::Language,
                match_pattern: "shell".to_string(),
                profile: "does-not-exist".to_string(),
            }],
            ..Default::default()
        };
        let config = RuntimeConfig {
            sandbox: Some(bad),
            ..Default::default()
        };
        let err = match Runtime::bootstrap(config).await {
            Ok(_) => panic!("invalid sandbox global config must fail bootstrap"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("Invalid sandbox global config"),
            "error: {err}"
        );
        assert!(err.to_string().contains("unknown profile"), "error: {err}");

        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_trigger_shutdown() {
        clear_env_vars();

        let config = default_test_config();
        let runtime = Runtime::bootstrap(config).await.unwrap();

        assert!(!runtime.is_shutting_down());
        runtime.trigger_shutdown();
        assert!(runtime.is_shutting_down());

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[test]
    fn test_runtime_config_default() {
        let config = RuntimeConfig::default();
        assert!(matches!(config.storage.storage_type, StorageType::Memory));
        assert!(config.mode_override.is_none());
        assert!(config.metrics.is_none());
    }

    #[tokio::test]
    async fn test_runtime_metrics_wiring() {
        clear_env_vars();

        let config = RuntimeConfig {
            metrics: Some(MetricsConfig {
                workflow_metrics: Some(wf_types::config::metrics::MetricCollectorConfig {
                    flush_interval: Some(100),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..default_test_config()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();

        let metrics = runtime
            .metrics()
            .expect("metrics system should be initialized");
        metrics.registry().workflow().record_execution_start("wf-1");
        assert_eq!(metrics.registry().workflow().usage_stats().total, 1);

        // Background flush task persists buffered metrics into storage.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        use wf_storage::adapter::metrics::MetricsStorageAdapter;
        let loaded = runtime
            .storage()
            .context()
            .unwrap()
            .metrics
            .query("workflow.execution.count", 0, wf_common::now() + 1000)
            .await
            .unwrap();
        assert_eq!(loaded.len(), 1);

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_runtime_metrics_disabled() {
        clear_env_vars();

        let config = RuntimeConfig {
            metrics: Some(MetricsConfig {
                enabled: Some(false),
                ..Default::default()
            }),
            ..default_test_config()
        };

        let runtime = Runtime::bootstrap(config).await.unwrap();
        assert!(runtime.metrics().is_none());

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_execution_callback_wired_call_agent_via_tool() {
        clear_env_vars();

        let config = default_test_config();
        let runtime = Runtime::bootstrap(config).await.unwrap();

        register_agent_tools(&runtime);
        mock_llm_gateway(&runtime, "mock", "agent answer");

        let (ctx, options) = default_tool_context();

        // call_agent through the shared tool registry hits the composite
        // callback (previously CallbackNotRegistered in production).
        let result = runtime
            .tool_registry()
            .execute_tool(
                "call_agent",
                &serde_json::json!({
                    "agent_id": "integration-agent",
                    "agent_profile_id": "mock",
                    "prompt": "hello",
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(result.success, "call_agent failed: {:?}", result.error);
        assert_eq!(result.result.unwrap()["result"], "agent answer");

        // The execution is registered in the shared agent loop registry and
        // observable through the query_workflow_status tool with its result.
        let ids = runtime.api_context().agent_loops.get_all_ids();
        assert_eq!(ids.len(), 1, "execution must be registered");
        let status = query_status_via_tool(&runtime, &ids[0].to_string()).await;
        assert_eq!(status["status"], "completed");
        assert_eq!(status["result"], "agent answer");

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_execution_callback_call_agent_wait_false_spawns() {
        clear_env_vars();

        let config = default_test_config();
        let runtime = Runtime::bootstrap(config).await.unwrap();

        register_agent_tools(&runtime);
        mock_llm_gateway(&runtime, "mock", "async answer");

        let (ctx, options) = default_tool_context();

        let result = runtime
            .tool_registry()
            .execute_tool(
                "call_agent",
                &serde_json::json!({
                    "agent_id": "integration-agent",
                    "agent_profile_id": "mock",
                    "prompt": "hello",
                    "wait": false,
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(
            result.success,
            "spawned call_agent failed: {:?}",
            result.error
        );
        let value = result.result.unwrap();
        assert_eq!(value["status"], "started");
        let execution_id = value["execution_id"].as_str().unwrap().to_string();

        // The spawned execution progresses in the background; polling the
        // query tool eventually returns the result.
        let status = poll_until_completed(&runtime, &execution_id).await;
        assert_eq!(status["result"], "async answer");

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[tokio::test]
    async fn test_execution_callback_execute_workflow_via_tool() {
        clear_env_vars();

        let config = default_test_config();
        let runtime = Runtime::bootstrap(config).await.unwrap();

        register_workflow_tools(&runtime);

        // The llm_summary_workflow LLM node uses the DEFAULT profile.
        let mock = Arc::new(wf_llm::mock::MockLlmClient::new());
        mock.default(wf_llm::mock::LlmResponseSpec::text("compressed").with_usage(50, 30));
        runtime.llm_gateway().register_mock("DEFAULT", mock);

        let (ctx, options) = default_tool_context();

        let message = wf_types::message::Message {
            id: wf_common::generate_id(),
            role: wf_types::message::MessageRole::User,
            content: wf_types::message::MessageContentValue::Text("long context".to_string()),
            timestamp: wf_common::now(),
            tool_call_id: None,
            tool_name: None,
            tool_calls: None,
            thinking: None,
            metadata: None,
        };
        let result = runtime
            .tool_registry()
            .execute_tool(
                "execute_workflow",
                &serde_json::json!({
                    "workflow_id": "llm_summary_workflow",
                    "input": {
                        "conversationHistory": [
                            serde_json::to_value(&message).unwrap()
                        ]
                    }
                }),
                &options,
                &ctx,
            )
            .await
            .unwrap();
        assert!(
            result.success,
            "execute_workflow failed: {:?}",
            result.error
        );
        let value = result.result.unwrap();
        let execution_id = value["execution_id"].as_str().unwrap().to_string();

        // Registered resource workflows are resolvable; the execution
        // completes and its status is queryable through the query tool.
        let mut terminal = false;
        for _ in 0..200 {
            let status = query_status_via_tool(&runtime, &execution_id).await;
            let state = status["status"].as_str().unwrap_or_default().to_string();
            if state == "completed" || state == "failed" {
                terminal = true;
                assert_eq!(state, "completed", "status: {status:?}");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(terminal, "workflow execution did not settle");

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }

    #[test]
    fn test_adjust_log_config_json_mode() {
        use crate::logger::LogFormat;

        let mode = ModeInfo {
            mode: ExecutionMode::Headless,
            output_format: crate::mode::OutputFormat::Json,
            color_enabled: false,
        };

        let config = LogConfig::default().with_format(LogFormat::Full);
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.format, LogFormat::Json);
    }

    #[test]
    fn test_adjust_log_config_silent_mode() {
        let mode = ModeInfo {
            mode: ExecutionMode::Interactive,
            output_format: crate::mode::OutputFormat::Silent,
            color_enabled: false,
        };

        let config = LogConfig::default().with_level("info");
        let adjusted = adjust_log_config(config, &mode);

        assert_eq!(adjusted.level, "off");
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
            provider: wf_types::llm::LlmProvider::OpenaiChat,
            model: "gpt-4o".to_string(),
            api_key: None,
            base_url: None,
            parameters: None,
            timeout: None,
            max_retries: None,
            retry_delay: None,
            headers: None,
            metadata: None,
            tool_call_format: None,
            auth_type: None,
            custom_headers: None,
            custom_body: None,
            custom_body_enabled: None,
            query_params: None,
            stream_options: None,
            context_window_size: None,
        }];
        let gateway = init_llm_gateway(&LlmConfig { profiles }, None).unwrap();
        assert!(gateway.has_profile("openai"));

        let err = match init_llm_gateway(
            &LlmConfig {
                profiles: vec![wf_types::llm::LlmProfile {
                    id: String::new(),
                    name: "broken".to_string(),
                    provider: wf_types::llm::LlmProvider::OpenaiChat,
                    model: String::new(),
                    api_key: None,
                    base_url: None,
                    parameters: None,
                    timeout: None,
                    max_retries: None,
                    retry_delay: None,
                    headers: None,
                    metadata: None,
                    tool_call_format: None,
                    auth_type: None,
                    custom_headers: None,
                    custom_body: None,
                    custom_body_enabled: None,
                    query_params: None,
                    stream_options: None,
                    context_window_size: None,
                }],
            },
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
        let ctx =
            wf_tools::executor::trait_def::ToolExecutionContext::new("exec-bridge".into());
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
        let mut saw_created = false;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while std::time::Instant::now() < deadline {
            match sub.try_recv() {
                Ok(event) => {
                    if event.r#type == wf_types::events::EventType::ShellSessionCreated {
                        saw_created = true;
                        assert_eq!(
                            event.metadata.unwrap()["session_id"],
                            result.result.unwrap()["session_id"]
                        );
                        break;
                    }
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
        assert!(
            saw_created,
            "no ShellSessionCreated event on the runtime EventBus"
        );

        runtime.shutdown().await.unwrap();
        clear_env_vars();
    }
}
