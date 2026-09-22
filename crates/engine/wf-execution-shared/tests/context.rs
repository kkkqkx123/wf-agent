use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use serde_json::{json, Value};
use wf_core::internal_signal::InternalSignalBus;
use wf_execution_shared::{
    ExecutorContext, NodeExecutionContext, NodeExecutionResult, NodeInputShape,
};
use wf_tools::registry::ToolRegistry;
use wf_types::node::StaticNodeType;
use wf_types::workflow_execution::WorkflowExecutionOptions;

fn test_options() -> WorkflowExecutionOptions {
    WorkflowExecutionOptions {
        input: None,
        max_steps: None,
        timeout: None,
        max_execution_time: None,
        enable_checkpoints: None,
        node_timeout: None,
        max_pause_duration: None,
        max_navigation_multiplier: None,
        loop_max_iterations_cap: None,
    }
}

fn test_executor_context() -> ExecutorContext {
    ExecutorContext::new(
        "exec-1".to_string(),
        "wf-1".to_string(),
        None,
        Arc::new(ToolRegistry::new()),
        test_options(),
    )
}

fn test_node_context() -> NodeExecutionContext {
    NodeExecutionContext::new(
        "exec-1".to_string(),
        "node-1".to_string(),
        StaticNodeType::Variable,
        json!({"key": "value"}),
        Arc::new(DashMap::new()),
    )
}

#[test]
fn executor_context_new_sets_defaults() {
    let ctx = test_executor_context();
    assert_eq!(ctx.execution_id, "exec-1");
    assert_eq!(ctx.workflow_id, "wf-1");
    assert!(ctx.variables.is_empty());
    assert!(ctx.parent_execution_id.is_none());
    assert!(ctx.metrics.is_none());
    assert!(ctx.token_tracker.is_some());
    assert!(ctx.hook_handler_registry.is_none());
    assert!(ctx.tool_approval_handler.is_none());
    assert!(ctx.readonly_variables.is_none());
    assert!(ctx.signal_bus.is_none());
}

#[tokio::test]
async fn executor_context_with_token_limit_updates_tracker() {
    let ctx = test_executor_context().with_token_limit(1_000);
    let tracker = ctx.token_tracker.as_ref().expect("tracker present");
    assert_eq!(tracker.lock().await.token_limit(), 1_000);
}

#[test]
fn executor_context_builder_chain_sets_parent_and_readonly() {
    let mut names = HashSet::new();
    names.insert("api_key".to_string());
    let ctx = test_executor_context()
        .with_parent_execution("parent-1".to_string())
        .with_readonly_variables(Arc::new(names));
    assert_eq!(ctx.parent_execution_id.as_deref(), Some("parent-1"));
    let readonly = ctx.readonly_variables.expect("readonly set");
    assert!(readonly.contains("api_key"));
}

#[test]
fn executor_context_with_signal_bus_injects_bus() {
    let ctx = test_executor_context().with_signal_bus(Arc::new(InternalSignalBus::new()));
    assert!(ctx.signal_bus.is_some());
}

#[test]
fn node_context_variable_guard_rejects_internal_prefix() {
    let ctx = test_node_context();
    ctx.set_variable("greeting", json!("hello"))
        .expect("public write succeeds");
    assert_eq!(ctx.get_variable("greeting"), Some(json!("hello")));
    let err = ctx
        .set_variable("__loop_stack", json!([]))
        .expect_err("internal prefix rejected");
    assert!(err.to_string().contains("__loop_stack"));
    assert!(ctx.get_variable("__loop_stack").is_none());
}

#[test]
fn node_context_internal_variable_bypass_writes_engine_state() {
    let ctx = test_node_context();
    ctx.set_internal_variable("__loop_stack", json!([1]));
    assert_eq!(ctx.get_variable("__loop_stack"), Some(json!([1])));
}

#[test]
fn node_context_builder_chain_sets_identity_fields() {
    let cache = Arc::new(std::sync::Mutex::new(HashMap::<String, Value>::new()));
    let ctx = test_node_context()
        .with_node_name("greet")
        .with_node_config(json!({"mode": "strict"}))
        .with_depth(3)
        .with_parent_execution("parent-1".to_string())
        .with_session_cache(cache);
    assert_eq!(ctx.node_name.as_deref(), Some("greet"));
    assert_eq!(ctx.node_config, Some(json!({"mode": "strict"})));
    assert_eq!(ctx.depth, 3);
    assert_eq!(ctx.parent_execution_id.as_deref(), Some("parent-1"));
    assert!(ctx.session_cache.is_some());
    assert_eq!(ctx.input_shape, NodeInputShape::None);
}

#[test]
fn node_execution_result_helpers_build_outputs() {
    let simple = NodeExecutionResult::simple(json!({"ok": true}));
    assert_eq!(simple.output, json!({"ok": true}));
    assert!(simple.next_node_ids.is_empty());

    let branched =
        NodeExecutionResult::with_next_nodes(json!(1), vec!["b".to_string(), "c".to_string()]);
    assert_eq!(
        branched.next_node_ids,
        vec!["b".to_string(), "c".to_string()]
    );
}
