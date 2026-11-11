#[cfg(feature = "plugins")]
use crate::bootstrap::PluginConfig;
use crate::bootstrap::{
    adjust_log_config, init_llm_gateway, resolve_infra_config, storage_db_path, InfraSourceConfig,
    LlmConfig, McpRuntimeConfig, ResourceConfig, Runtime, RuntimeConfig,
};
use crate::logger::LogConfig;
use crate::mode::{ExecutionMode, ModeInfo};
use std::path::PathBuf;
use std::sync::Arc;
use wf_core::registry::Registry;
use wf_types::config::metrics::MetricsConfig;
use wf_types::config::storage::{StorageConfig, StorageType};

fn clear_env_vars() {
    std::env::remove_var("CLI_MODE");
    std::env::remove_var("HEADLESS");
    std::env::remove_var("TEST_MODE");
    std::env::remove_var("CLI_OUTPUT_FORMAT");
    std::env::remove_var("NO_COLOR");
    std::env::remove_var("WF_KEYWORD_DB_PATH");
    std::env::remove_var("WF_KEYWORD_ENABLED");
    std::env::remove_var("WF_KEYWORD_BACKEND");
    std::env::remove_var("WF_KEYWORD_TANTIVY_PATH");
    std::env::remove_var("WF_KEYWORD_ES_URL");
    std::env::remove_var("WF_KEYWORD_ES_API_KEY");
    std::env::remove_var("WF_KEYWORD_ES_INDEX");
    std::env::remove_var("WF_KEYWORD_ES_TIMEOUT_SECS");
    std::env::remove_var("WF_KEYWORD_ES_BATCH_SIZE");
    std::env::remove_var("WF_KEYWORD_ES_EXACT_COUNT");
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
    // A real execution id stands in for the calling execution: the
    // control-plane tools scope queries to the caller's subtree, and
    // `dispatch_tool_context` roots the tree this id owns.
    let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("test-host".into());
    let options = wf_types::tool::ToolExecutionOptions {
        timeout: None,
        retries: None,
        retry_delay: None,
        exponential_backoff: None,
    };
    (ctx, options)
}

/// Tool context carrying a live parent link rooted at the host execution id,
/// so dispatched executions join the same tree the query context belongs to,
/// exactly like production tool calls inside a running execution.
fn dispatch_tool_context() -> (
    wf_tools::executor::trait_def::ToolExecutionContext,
    wf_types::tool::ToolExecutionOptions,
) {
    let manager = Arc::new(wf_core::hierarchy::ExecutionHierarchyManager::new(
        wf_types::Id::from("test-host".to_string()),
        wf_types::execution::ExecutionType::Agent,
    ));
    let link = wf_tools::callback::ParentLink {
        file_scope: wf_types::config::file_scope::ResolvedExecutionScope::none(),
        execution_id: wf_types::Id::from("test-host".to_string()),
        manager,
        cancellation: None,
        tool_call_id: None,
        subagent_tx: None,
    };
    let ctx = wf_tools::executor::trait_def::ToolExecutionContext::new("test-host".into())
        .with_parent_link(Some(link));
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
    assert!(
        wf_common::poll_until(
            std::time::Duration::from_millis(10),
            std::time::Duration::from_secs(2),
            || async {
                query_status_via_tool(runtime, execution_id).await["status"] == "completed"
            },
        )
        .await,
        "execution {execution_id} did not complete within poll window"
    );
    query_status_via_tool(runtime, execution_id).await
}

mod execution_callback;
mod infra_config;
mod keyword;
mod metrics_wiring;
mod runtime_bootstrap;
mod sandbox;
mod service_wiring;
