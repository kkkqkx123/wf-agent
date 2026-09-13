//! Unit tests for the tool-execution coordinator split across
//! `tool.rs`, `tool/runner.rs`, `tool/approval.rs` and `tool/general.rs`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use wf_tools::registry::ToolRegistry;
use wf_types::message::{Message, MessageContentValue};
use wf_types::tool::{CheckpointTiming, Tool, ToolRiskLevel};
use wf_types::Id;

use super::approval::ToolApprovalGate;
use super::general::GeneralToolContext;
use super::types::{
    ToolCheckpointHandler, ToolExecutionMode, ToolProgressStatus, ToolRunCtx, ToolVisibilityStore,
};
use super::ToolExecutionCoordinator;
use crate::approval::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use crate::entity::AgentLoopEntity;
use crate::error::AgentResult;

fn mock_tool_registry(executed: &Arc<AtomicU32>) -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    let handler: wf_tools::executor::stateless::StatelessHandler = {
        let executed = executed.clone();
        Arc::new(
            move |_params: &Value, _ctx: &wf_tools::executor::trait_def::ToolExecutionContext| {
                executed.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("tool-result-ok"))
            },
        )
    };
    registry.register_tool(Tool {
        id: "tool-1".to_string(),
        name: "mock_write".to_string(),
        description: "mock tool".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: Some("mock".to_string()),
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::Write),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_handler("tool-1", handler);
    registry
}

fn make_tool_call(id: &str, name: &str) -> wf_types::message::LlmToolCall {
    wf_types::message::LlmToolCall {
        id: id.to_string(),
        r#type: "function".to_string(),
        function: wf_types::message::LlmFunctionCall {
            name: name.to_string(),
            arguments: "{}".to_string(),
        },
    }
}

fn text_of(msg: &Message) -> String {
    match &msg.content {
        MessageContentValue::Text(t) => t.clone(),
        MessageContentValue::Rich(_) => String::new(),
    }
}

fn make_entity() -> AgentLoopEntity {
    AgentLoopEntity::new(Id::from("agent-approval-1".to_string()))
}

#[tokio::test]
async fn test_no_handler_auto_approves_and_executes() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry);
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("tool execution must succeed");

    assert_eq!(messages.len(), 1);
    assert_eq!(executed.load(Ordering::SeqCst), 1);
    assert!(text_of(&messages[0]).contains("tool-result-ok"));
}

struct RejectingHandler {
    reason: String,
}

#[async_trait::async_trait]
impl ToolApprovalHandler for RejectingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult::rejected(request.tool_call_id.clone(), self.reason.clone())
    }
}

struct ApprovingHandler;

#[async_trait::async_trait]
impl ToolApprovalHandler for ApprovingHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        ToolApprovalResult::approved(request.tool_call_id.clone())
    }
}

#[tokio::test]
async fn test_rejecting_handler_blocks_tool_and_produces_message() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry).with_approval(
        None,
        Some(Arc::new(RejectingHandler {
            reason: "too risky".to_string(),
        })),
    );
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("rejection must not fail the loop");

    // Tool never executed; a rejection tool message is produced.
    assert_eq!(executed.load(Ordering::SeqCst), 0);
    assert_eq!(messages.len(), 1);
    let content = text_of(&messages[0]);
    assert!(content.contains("too risky"));
    assert!(content.contains("mock_write"));
}

struct VetoHandler;

#[async_trait::async_trait]
impl wf_execution_shared::hooks::HookHandler for VetoHandler {
    fn name(&self) -> &str {
        "gate"
    }
    async fn on_point(
        &self,
        _ctx: &wf_execution_shared::hooks::HookContext,
    ) -> wf_execution_shared::hooks::HookOutcome {
        wf_execution_shared::hooks::HookOutcome::Veto {
            reason: "unverified checksum".to_string(),
        }
    }
}

#[tokio::test]
async fn test_before_tool_call_veto_denies_like_rejection() {
    use wf_execution_shared::hooks::HookHandlerRegistry;

    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let hook_registry = Arc::new(HookHandlerRegistry::new());
    assert!(hook_registry.register("BEFORE_TOOL_CALL", Arc::new(VetoHandler), 1));
    let coordinator =
        ToolExecutionCoordinator::new(registry).with_hook_handler_registry(Some(hook_registry));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("veto must not fail the loop");

    // Tool never executed; the denial surfaces as a rejection message
    // carrying the veto reason.
    assert_eq!(executed.load(Ordering::SeqCst), 0);
    assert_eq!(messages.len(), 1);
    let content = text_of(&messages[0]);
    assert!(
        content.contains("hook veto at BEFORE_TOOL_CALL"),
        "{content}"
    );
    assert!(content.contains("unverified checksum"), "{content}");
    assert!(content.contains("mock_write"), "{content}");
}

#[tokio::test]
async fn test_approving_handler_allows_execution() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry)
        .with_approval(None, Some(Arc::new(ApprovingHandler)));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("approved tool must execute");

    assert_eq!(executed.load(Ordering::SeqCst), 1);
    assert_eq!(messages.len(), 1);
    assert!(text_of(&messages[0]).contains("tool-result-ok"));
}

#[tokio::test]
async fn test_replayed_tool_call_id_is_served_from_cache() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry);
    let entity = make_entity();

    // First execution of the call id runs the tool and caches the result.
    let first = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-replay", "mock_write")])
        .await
        .expect("first execution succeeds");
    assert_eq!(executed.load(Ordering::SeqCst), 1);
    assert!(text_of(&first[0]).contains("tool-result-ok"));

    // A replayed call with the same id (crash/restore scenario) must NOT
    // re-execute the tool: the cached result is returned instead.
    let second = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-replay", "mock_write")])
        .await
        .expect("replay succeeds");
    assert_eq!(
        executed.load(Ordering::SeqCst),
        1,
        "tool must not run twice for the same call id"
    );
    assert!(text_of(&second[0]).contains("tool-result-ok"));
}

#[tokio::test]
async fn test_in_flight_marker_cleared_after_execution() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry);
    let entity = make_entity();
    entity.state.write().await.start_iteration();

    coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-live", "mock_write")])
        .await
        .unwrap();

    let state = entity.state.read().await;
    assert!(
        state.pending_tool_calls().is_empty(),
        "no call remains in flight after execution"
    );
    assert!(state.has_completed_tool_call("tc-live"));
    let recorded = state.iteration_history()[0]
        .tool_calls
        .iter()
        .find(|t| t.tool_call_id.as_deref() == Some("tc-live"));
    assert!(recorded.is_some(), "tool call id recorded in audit trail");
}

#[tokio::test]
async fn test_parallel_approval_no_crosstalk() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    // Second tool with the same underlying counter: only approved calls
    // reach execution.
    let registry2 = registry.clone();
    registry2.register_tool(Tool {
        id: "tool-2".to_string(),
        name: "mock_read".to_string(),
        description: "mock read".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: Some("mock".to_string()),
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::ReadOnly),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    {
        let handler: wf_tools::executor::stateless::StatelessHandler = {
            let executed = executed.clone();
            Arc::new(
                move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                    executed.fetch_add(1, Ordering::SeqCst);
                    Ok(Value::from("tool-result-ok"))
                },
            )
        };
        registry2.register_stateless_handler("tool-2", handler);
    }
    let handler_executed = Arc::new(AtomicU32::new(0));
    let handler_executed_clone = handler_executed.clone();
    let handler = Arc::new(move |request: &ToolApprovalRequest| {
        handler_executed_clone.fetch_add(1, Ordering::SeqCst);
        ToolApprovalResult::approved(request.tool_call_id.clone())
    });

    struct FnHandler(Arc<dyn Fn(&ToolApprovalRequest) -> ToolApprovalResult + Send + Sync>);

    #[async_trait::async_trait]
    impl ToolApprovalHandler for FnHandler {
        async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
            (self.0)(request)
        }
    }

    let coordinator = ToolExecutionCoordinator::new(registry.clone())
        .with_mode(ToolExecutionMode::Parallel)
        .with_approval(None, Some(Arc::new(FnHandler(handler))));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(
            &entity,
            &[
                make_tool_call("tc-1", "mock_write"),
                make_tool_call("tc-2", "mock_read"),
            ],
        )
        .await
        .expect("parallel approval must not fail");

    assert_eq!(messages.len(), 2);
    // Both asked the handler (no auto approval with handler present).
    assert_eq!(handler_executed.load(Ordering::SeqCst), 2);
    // Both approved and executed exactly once.
    assert_eq!(executed.load(Ordering::SeqCst), 2);
    assert!(messages
        .iter()
        .all(|m| text_of(m).contains("tool-result-ok")));
}

// ---- orchestration enhancements ----

#[tokio::test]
async fn test_predefined_read_file_auto_approved_under_safe_preset() {
    let registry = Arc::new(ToolRegistry::new());
    let tool = wf_tools::predefined::filesystem::READ_FILE.tool_def();
    let tool_name = tool.name.clone();
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Ok(Value::from(format!("content of {}", tool_name)))
        },
    );
    registry.register_tool(tool);
    registry.register_stateless_handler("read_file", handler);

    let options = wf_types::tool::approval::ToolApprovalOptions {
        auto_approval_enabled: Some(true),
        security_preset: Some(wf_types::tool::approval::SecurityPreset::Safe),
        risk_threshold: None,
        auto_approve_patterns: None,
        categories: None,
        workspace_boundary: None,
        file_permissions: None,
        command: None,
        mcp: None,
        network: None,
        interaction: None,
        allow_write_protected: None,
    };
    let coordinator = ToolExecutionCoordinator::new(registry).with_approval(Some(options), None);
    let entity = make_entity();

    let mut tc = make_tool_call("tc-read", "read_file");
    tc.function.arguments = serde_json::json!({ "path": "/tmp/readme.md" }).to_string();

    let messages = coordinator
        .execute_tool_calls(&entity, &[tc])
        .await
        .expect("read-only tool must be auto-approved and executed");

    assert_eq!(messages.len(), 1);
    assert!(text_of(&messages[0]).contains("content of read_file"));
}

#[tokio::test]
async fn test_progress_events_emitted() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let coordinator = ToolExecutionCoordinator::new(registry).with_progress_tx(Some(tx));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("execution must succeed");

    assert_eq!(messages.len(), 1);
    assert_eq!(executed.load(Ordering::SeqCst), 1);
    let mut statuses = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        assert_eq!(ev.tool_call_id, "tc-1");
        statuses.push(ev.status);
    }
    assert_eq!(
        statuses,
        vec![ToolProgressStatus::Started, ToolProgressStatus::Completed]
    );
}

struct BlockingVisibilityStore;

#[async_trait::async_trait]
impl ToolVisibilityStore for BlockingVisibilityStore {
    async fn is_tool_visible(&self, _execution_id: &str, _tool_name: &str) -> bool {
        false
    }
}

#[tokio::test]
async fn test_visibility_gate_blocks_tool() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let coordinator = ToolExecutionCoordinator::new(registry)
        .with_visibility_store(Some(Arc::new(BlockingVisibilityStore)));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("visibility rejection must not fail the loop");

    assert_eq!(executed.load(Ordering::SeqCst), 0);
    assert_eq!(messages.len(), 1);
    assert!(text_of(&messages[0]).contains("not visible"));
}

fn mock_failing_registry() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Err(wf_tools::error::ToolError::ExecutionFailed {
                tool_id: "fail-id".to_string(),
                reason: "boom".to_string(),
            })
        },
    );
    registry.register_tool(Tool {
        id: "fail-id".to_string(),
        name: "mock_fail".to_string(),
        description: "mock failing tool".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: Some("mock".to_string()),
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::Write),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_handler("fail-id", handler);
    registry
}

#[tokio::test]
async fn test_failure_protection_blocks_after_consecutive_failures() {
    let registry = mock_failing_registry();
    let protection = Arc::new(
        wf_tools::failure_protection::ToolFailureProtectionState::new(
            wf_tools::failure_protection::ToolFailureProtectionConfig {
                max_consecutive_failures: 2,
                cooldown_period: Duration::from_secs(60),
                enabled: true,
            },
        ),
    );
    let coordinator =
        ToolExecutionCoordinator::new(registry).with_failure_protection(Some(protection.clone()));
    let entity = make_entity();

    // First two executions are allowed and record failures.
    for _ in 0..2 {
        let messages = coordinator
            .execute_tool_calls(&entity, &[make_tool_call("tc-x", "mock_fail")])
            .await
            .expect("execution must not fail");
        assert_eq!(messages.len(), 1);
    }
    assert!(protection.is_blocked("mock_fail"));

    // The third execution is blocked by the protection gate.
    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-y", "mock_fail")])
        .await
        .expect("blocked execution must not fail");
    assert_eq!(messages.len(), 1);
    assert!(text_of(&messages[0]).contains("blocked"));
}

struct CountingCheckpointHandler {
    before: Arc<AtomicU32>,
    after: Arc<AtomicU32>,
}

#[async_trait::async_trait]
impl ToolCheckpointHandler for CountingCheckpointHandler {
    async fn create_checkpoint(&self, _execution_id: &str, reason: &str) -> AgentResult<()> {
        if reason.starts_with("before") {
            self.before.fetch_add(1, Ordering::SeqCst);
        } else {
            self.after.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[tokio::test]
async fn test_checkpoint_before_and_after() {
    let executed = Arc::new(AtomicU32::new(0));
    let registry = mock_tool_registry(&executed);
    let mut tool = registry.get_tool("tool-1").unwrap();
    tool.metadata.as_mut().unwrap().create_checkpoint = Some(CheckpointTiming::Both);
    registry.register_tool(tool);

    let before = Arc::new(AtomicU32::new(0));
    let after = Arc::new(AtomicU32::new(0));
    let handler = Arc::new(CountingCheckpointHandler {
        before: before.clone(),
        after: after.clone(),
    });
    let coordinator =
        ToolExecutionCoordinator::new(registry).with_checkpoint_handler(Some(handler));
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(&entity, &[make_tool_call("tc-1", "mock_write")])
        .await
        .expect("checkpoint-enabled execution must succeed");

    assert_eq!(messages.len(), 1);
    assert_eq!(executed.load(Ordering::SeqCst), 1);
    assert_eq!(before.load(Ordering::SeqCst), 1);
    assert_eq!(after.load(Ordering::SeqCst), 1);
}

fn mock_mixed_registry() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    registry.register_tool(Tool {
        id: "fail-id".to_string(),
        name: "mock_fail".to_string(),
        description: "mock failing tool".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: Some("mock".to_string()),
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::Write),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_handler(
        "fail-id",
        Arc::new(
            move |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
                Err(wf_tools::error::ToolError::ExecutionFailed {
                    tool_id: "fail-id".to_string(),
                    reason: "boom".to_string(),
                })
            },
        ),
    );
    registry.register_tool(Tool {
        id: "slow-id".to_string(),
        name: "mock_slow".to_string(),
        description: "slow mock tool".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: Some("mock".to_string()),
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::ReadOnly),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: None,
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_async_handler(
        "slow-id",
        Arc::new(
            |_p: Value, _c: wf_tools::executor::trait_def::ToolExecutionContext| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    Ok(Value::from("tool-result-ok"))
                })
            },
        ),
    );
    registry
}

#[tokio::test]
async fn test_parallel_cancel_on_failure_aborts_batch() {
    let registry = mock_mixed_registry();
    let coordinator = ToolExecutionCoordinator::new(registry)
        .with_mode(ToolExecutionMode::Parallel)
        .with_cancel_on_failure(true);
    let entity = make_entity();

    let messages = coordinator
        .execute_tool_calls(
            &entity,
            &[
                make_tool_call("tc-fail", "mock_fail"),
                make_tool_call("tc-slow", "mock_slow"),
            ],
        )
        .await
        .expect("parallel execution must not fail");

    assert_eq!(messages.len(), 2);
    let texts: Vec<String> = messages.iter().map(text_of).collect();
    assert!(
        texts.iter().any(|t| t.contains("boom")),
        "failing tool must surface its error: {:?}",
        texts
    );
    assert!(
        texts.iter().any(|t| t.contains("did not complete")),
        "aborted tool must be reported: {:?}",
        texts
    );
}

// ── general tool invoker ─────────────────────────────────────────

use wf_tools::general::GeneralToolInvoker;

fn echo_registry() -> Arc<ToolRegistry> {
    let registry = Arc::new(ToolRegistry::new());
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        |params: &Value, _ctx: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Ok(serde_json::json!({ "echo": params }))
        },
    );
    registry.register_tool(Tool {
        id: "web_search".to_string(),
        name: "web_search".to_string(),
        description: "Search the web".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: Some(wf_types::tool::ToolParameterSchema {
            r#type: "object".to_string(),
            properties: std::collections::BTreeMap::from([(
                "query".to_string(),
                wf_types::tool::ToolPropertySchema::typed("string"),
            )]),
            required: vec!["query".to_string()],
            additional_properties: Some(false),
        }),
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    registry.register_stateless_handler("web_search", handler);
    registry
}

fn general_entity(registry: &ToolRegistry) -> Arc<AgentLoopEntity> {
    let entity = Arc::new(
        AgentLoopEntity::new(Id::from("exec-general-1".to_string()))
            .with_available_tool_names(vec!["web_search".to_string(), "write_file".to_string()])
            .with_initial_tool_names(vec!["web_search".to_string()])
            .with_discoverable_tool_names(vec!["web_search".to_string()]),
    );
    // Ensure the inner tool is registered (mirrors the pipeline).
    assert!(registry.list_tools().iter().any(|t| t.name == "web_search"));
    entity
}

fn general_ctx(registry: Arc<ToolRegistry>, entity: Arc<AgentLoopEntity>) -> GeneralToolContext {
    let run_ctx = ToolRunCtx {
        registry,
        metrics: None,
        progress_tx: None,
        checkpoint_handler: None,
        failure_protection: None,
        visibility_store: None,
        general_invoker: None,
        retry_budget: None,
        checkpoint_session: None,
    };
    GeneralToolContext::new(run_ctx, entity, None)
}

#[tokio::test]
async fn test_general_invoke_returns_inner_tool_native_result() {
    let registry = echo_registry();
    let entity = general_entity(&registry);
    let ctx = general_ctx(registry, entity);

    let result = ctx
        .invoke_request("{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust 异步\"}}")
        .await
        .expect("general invoke must succeed");
    assert_eq!(
        result,
        serde_json::json!({ "echo": { "query": "rust 异步" } })
    );
}

#[tokio::test]
async fn test_general_parse_error_returns_format_hint() {
    let registry = echo_registry();
    let entity = general_entity(&registry);
    let ctx = general_ctx(registry, entity);

    for request in ["", "plain text", "{\"tool\": 123}"] {
        let err = ctx.invoke_request(request).await.unwrap_err();
        assert!(
            err.to_string().contains("\"tool\""),
            "parse errors must carry the format hint: {err}"
        );
    }
}

#[tokio::test]
async fn test_general_rejects_hidden_and_non_whitelisted_tools() {
    let registry = echo_registry();
    registry.register_tool(Tool {
        id: "secret_admin".to_string(),
        name: "secret_admin".to_string(),
        description: "hidden admin".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Ok(Value::from("admin-ok"))
        },
    );
    registry.register_stateless_handler("secret_admin", handler);

    let entity = Arc::new(
        AgentLoopEntity::new(Id::from("exec-general-2".to_string()))
            .with_available_tool_names(vec!["web_search".to_string()])
            .with_initial_tool_names(vec!["web_search".to_string()])
            .with_discoverable_tool_names(vec!["web_search".to_string()])
            .with_hidden_tool_names(vec!["secret_admin".to_string()]),
    );
    let ctx = general_ctx(registry, entity);

    let hidden = ctx
        .invoke_request("{\"tool\": \"secret_admin\", \"parameters\": {\"x\": 1}}")
        .await
        .unwrap_err();
    assert!(hidden.to_string().contains("not callable"));

    let outside = ctx
        .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a\"}}")
        .await
        .unwrap_err();
    assert!(outside
        .to_string()
        .contains("not in the available tool set"));
}

#[tokio::test]
async fn test_general_rejects_gated_tool_until_activated() {
    // write_file is gated: available but neither initial nor discoverable.
    let registry = echo_registry();
    registry.register_tool(Tool {
        id: "write_file".to_string(),
        name: "write_file".to_string(),
        description: "write".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Ok(Value::from("written"))
        },
    );
    registry.register_stateless_handler("write_file", handler);

    let entity = general_entity(&registry);
    let ctx = general_ctx(registry, entity.clone());

    let before = ctx
        .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}")
        .await
        .unwrap_err();
    assert!(before.to_string().contains("not activated"));

    // Formal activation (TOOL_VISIBILITY unblock) allows the call.
    entity
        .state
        .write()
        .await
        .tool_discovery_mut()
        .activate_tool("write_file");
    let after = ctx
        .invoke_request("{\"tool\": \"write_file\", \"parameters\": {\"path\": \"a.txt\"}}")
        .await
        .expect("activated gated tool must be invokable");
    assert_eq!(after, serde_json::json!("written"));
}

#[tokio::test]
async fn test_general_rejects_self_invocation() {
    let registry = echo_registry();
    let entity = general_entity(&registry);
    let ctx = general_ctx(registry, entity);

    let err = ctx
        .invoke_request("{\"tool\": \"general\", \"parameters\": {\"request\": \"{}\"}}")
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("cannot be invoked through the general tool"),
        "self invocation must be rejected: {err}"
    );
}

#[tokio::test]
async fn test_general_invokes_metadata_discoverable_tool() {
    // Discoverability from tool metadata (not the config list) must be
    // honored by the runtime gate: the assembly injects the metadata and
    // enables `general`, so the call must not be rejected as gated.
    let registry = echo_registry();
    registry.register_tool(Tool {
        id: "beta_db".to_string(),
        name: "beta_db".to_string(),
        description: "db tool".to_string(),
        tool_type: wf_types::tool::ToolType::Stateless,
        parameters: None,
        metadata: Some(wf_types::tool::ToolMetadata {
            category: None,
            tags: None,
            documentation_url: None,
            custom_fields: None,
            risk_level: Some(ToolRiskLevel::ReadOnly),
            auto_approvable: None,
            create_checkpoint: None,
            exposure: Some(wf_types::tool::ToolExposure::Discoverable),
        }),
        config: None,
        enabled: Some(true),
        strict: None,
        default_timeout_ms: Some(5000),
    });
    let handler: wf_tools::executor::stateless::StatelessHandler = Arc::new(
        |_p: &Value, _c: &wf_tools::executor::trait_def::ToolExecutionContext| {
            Ok(Value::from("db-ok"))
        },
    );
    registry.register_stateless_handler("beta_db", handler);

    let entity = Arc::new(
        AgentLoopEntity::new(Id::from("exec-general-3".to_string()))
            .with_available_tool_names(vec!["web_search".to_string(), "beta_db".to_string()])
            .with_initial_tool_names(vec!["web_search".to_string()])
            .with_discoverable_tool_names(vec!["web_search".to_string()]),
    );
    let ctx = general_ctx(registry, entity);

    let result = ctx
        .invoke_request("{\"tool\": \"beta_db\", \"parameters\": {\"q\": 1}}")
        .await
        .expect("metadata-discoverable tool must be invokable via general");
    assert_eq!(result, serde_json::json!("db-ok"));
}

#[tokio::test]
async fn test_general_blocked_tool_rejected_by_pipeline() {
    let registry = echo_registry();
    let entity = general_entity(&registry);
    let run_ctx = ToolRunCtx {
        registry,
        metrics: None,
        progress_tx: None,
        checkpoint_handler: None,
        failure_protection: None,
        visibility_store: Some(Arc::new(BlockingVisibilityStore)),
        general_invoker: None,
        retry_budget: None,
        checkpoint_session: None,
    };
    let ctx = GeneralToolContext::new(run_ctx, entity, None);

    let err = ctx
        .invoke_request("{\"tool\": \"web_search\", \"parameters\": {\"query\": \"x\"}}")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not visible"));
}

#[tokio::test]
async fn test_general_records_discovery_state() {
    let registry = echo_registry();
    let entity = general_entity(&registry);
    let ctx = general_ctx(registry, entity.clone());

    let _ = ctx
        .invoke_request("{\"tool\": \"web_search\", \"parameters\": {\"query\": \"rust\"}}")
        .await
        .expect("invoke must succeed");

    let state = entity.state.read().await;
    assert!(state
        .tool_discovery()
        .discovered_via_general
        .contains("web_search"));
}

// Keep the approval gate import meaningful: the tests above exercise the
// gate through the coordinator; this assertion pins the wiring contract.
#[test]
fn test_approval_gate_config_roundtrip() {
    let gate = ToolApprovalGate::new(None, Some(Arc::new(ApprovingHandler)));
    let (options, handler) = gate.config();
    assert!(options.is_none());
    assert!(Arc::strong_count(&handler.expect("handler present")) >= 1);
}
