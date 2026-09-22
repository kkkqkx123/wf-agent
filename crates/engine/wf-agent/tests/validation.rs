//! Validation integration tests (`validation::AgentLoopValidator` +
//! executor gate): malformed configs are rejected before the loop starts.

use std::sync::Arc;

use wf_agent::validation::{AgentLoopValidator, ValidationSeverity};
use wf_agent::executor::AgentLoopExecutor;
use wf_llm::{LlmGateway, LlmResponseSpec, MockLlmClient};
use wf_tools::callback::{AgentLoopConfig, AgentLoopInput, HookConfig};
use wf_tools::registry::ToolRegistry;

fn base_config() -> AgentLoopConfig {
    AgentLoopConfig {
        agent_id: "agent-1".to_string(),
        model: "mock".to_string(),
        max_iterations: Some(5),
        max_execution_time: None,
        hooks: Vec::new(),
        available_tool_names: Vec::new(),
        initial_tool_names: Vec::new(),
        discoverable_tool_names: Vec::new(),
        enable_general_tool: None,
        activated_tool_names: Vec::new(),
        hidden_tool_names: Vec::new(),
        tool_call_protocol: None,
        token_limit: None,
        token_warning_threshold: None,
        enable_token_tracking: None,
        general_description: None,
        discoverable_metadata_block: None,
        history_normalization: false,
        checkpoint_message_interval: None,
    }
}

fn executor() -> AgentLoopExecutor {
    let gateway = Arc::new(LlmGateway::new());
    let mock = Arc::new(MockLlmClient::new());
    mock.default(LlmResponseSpec::text("done"));
    gateway.register_mock("mock", mock);
    let registry = Arc::new(ToolRegistry::new());
    AgentLoopExecutor::new(gateway, registry)
}

fn input() -> AgentLoopInput {
    AgentLoopInput {
        message: "run".to_string(),
        context: std::collections::HashMap::new(),
        conversation: Vec::new(),
    }
}

#[test]
fn valid_config_passes_without_issues() {
    let registry = ToolRegistry::new();
    let issues = AgentLoopValidator::validate_config(&base_config(), &registry);
    assert!(issues.is_empty());
}

#[test]
fn empty_agent_id_is_an_error() {
    let registry = ToolRegistry::new();
    let cfg = AgentLoopConfig {
        agent_id: "".to_string(),
        ..base_config()
    };
    let issues = AgentLoopValidator::validate_config(&cfg, &registry);
    assert!(issues
        .iter()
        .any(|i| i.field == "agent_id" && i.severity == ValidationSeverity::Error));
}

#[test]
fn unknown_tool_is_rejected() {
    let registry = ToolRegistry::new();
    let cfg = AgentLoopConfig {
        available_tool_names: vec!["nope_tool".to_string()],
        ..base_config()
    };
    let issues = AgentLoopValidator::validate_config(&cfg, &registry);
    assert!(issues.iter().any(|i| i.field == "available_tool_names"
        && i.severity == ValidationSeverity::Error));
}

#[test]
fn zero_iterations_rejected_and_over_cap_rejected() {
    let registry = ToolRegistry::new();
    let zero = AgentLoopConfig {
        max_iterations: Some(0),
        ..base_config()
    };
    assert!(AgentLoopValidator::validate_config(&zero, &registry)
        .iter()
        .any(|i| i.field == "max_iterations"));

    let over = AgentLoopConfig {
        max_iterations: Some(10),
        ..base_config()
    };
    assert!(AgentLoopValidator::validate_config_with_cap(&over, &registry, 5)
        .iter()
        .any(|i| i.field == "max_iterations"
            && i.severity == ValidationSeverity::Error));
}

#[test]
fn unknown_hook_type_is_rejected() {
    let registry = ToolRegistry::new();
    let cfg = AgentLoopConfig {
        hooks: vec![HookConfig {
            hook_type: "NOPE_HOOK".to_string(),
            condition: None,
            enabled: true,
            priority: 0,
            payload: None,
            handler: None,
            create_checkpoint: None,
            checkpoint_description: None,
        }],
        ..base_config()
    };
    let issues = AgentLoopValidator::validate_config(&cfg, &registry);
    assert!(issues
        .iter()
        .any(|i| i.field == "hooks" && i.severity == ValidationSeverity::Error));
}

#[tokio::test]
async fn executor_rejects_invalid_config_before_running() {
    let exec = executor();
    let bad = AgentLoopConfig {
        agent_id: "".to_string(),
        ..base_config()
    };
    let err = exec.execute(bad, input()).await.unwrap_err();
    assert!(
        err.to_string().contains("agent_id"),
        "executor gate must surface the field: {err}"
    );
}

#[tokio::test]
async fn executor_rejects_unknown_tool_before_running() {
    let exec = executor();
    let bad = AgentLoopConfig {
        available_tool_names: vec!["nope_tool".to_string()],
        ..base_config()
    };
    let err = exec.execute(bad, input()).await.unwrap_err();
    assert!(
        err.to_string().contains("nope_tool"),
        "executor gate must name the unknown tool: {err}"
    );
}

#[tokio::test]
async fn executor_enforces_max_iterations_cap() {
    let exec = executor().with_max_iterations_cap(3);
    let bad = AgentLoopConfig {
        max_iterations: Some(10),
        ..base_config()
    };
    let err = exec.execute(bad, input()).await.unwrap_err();
    assert!(
        err.to_string().contains("max_iterations"),
        "cap violation must be rejected: {err}"
    );
}
