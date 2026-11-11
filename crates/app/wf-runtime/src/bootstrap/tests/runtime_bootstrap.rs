use super::*;

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
        crate::bootstrap::default_storage_db_path("myapp")
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
async fn test_runtime_goal_review_resource_assembler_activation() {
    clear_env_vars();

    use wf_resource::registry::{RegisterOptions as ResourceOptions, ResourceAssemblerActivation};
    use wf_workflow::validation::GraphValidator;

    let config = RuntimeConfig {
        log_config: LogConfig::default().with_level("off"),
        resource: ResourceConfig {
            options: ResourceOptions {
                resource_assembler_activation: vec![ResourceAssemblerActivation {
                    id: "@standard/goal-review-agent".into(),
                    config: serde_json::json!({
                        "root_requirement": "fix the failing test",
                        "max_iterations": 3,
                    }),
                }],
                ..Default::default()
            },
            custom_source: None,
        },
        metrics: None,
        ..Default::default()
    };

    let runtime = Runtime::bootstrap(config).await.unwrap();

    // Built-in resource assembler registered and activated through the
    // unified plugin engine: the workflow lands in the registries via the
    // contribution bridge, and the planner node it wires resolves its system
    // prompt through the built-in `@standard` planner prompt template.
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
    // The built-in planner prompt is owned by the agent prompt registration
    // stage, not the assembler bundle, so deactivation leaves it in place.

    runtime.shutdown().await.unwrap();
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
