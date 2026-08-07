use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use wf_tools::approval::{ApprovalDecision, ToolApprovalCoordinator as EngineApprovalCoordinator};
use wf_types::events::{BaseEvent, EventType};
use wf_types::interaction::tool_approval::{ToolApprovalRequestData, ToolApprovalResponseData};
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::tool::ToolExecutionOptions;
use wf_types::UserInteractionStorageMetadata;

use crate::entity::user_interaction::save_interaction;
use crate::infra::context::ApiContext;
use crate::infra::error::{not_found, ApiError, ApiResult};

/// Tool approval decision for a single tool call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Auto-approved by policy; no human interaction was created.
    AutoApproved,
    /// Approved by a human responder.
    Approved,
    /// Rejected by a human responder (or a policy denial).
    Rejected,
    /// The approval request timed out waiting for a response.
    TimedOut,
}

/// Outcome of an approval round for one tool call (TS `ToolApprovalResult`
/// counterpart, extended with the batch/status context).
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalResult {
    pub status: ApprovalStatus,
    pub tool_call_id: String,
    pub tool_name: String,
    pub approved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_parameters: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_instruction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<String>,
}

/// Tool approval request/response flow (TS `ToolApprovalCoordinator`
/// counterpart).
///
/// The coordinator closes the approval loop at the API layer:
/// 1. policy check (auto-approval presets / patterns / file & command &
///    network rules) via `wf-tools`' approval engine;
/// 2. for tools that must be confirmed, a `user_interaction` record is
///    persisted (interaction_type `tool_approval`) and a wait channel is
///    registered on `wf-workflow`'s interaction registry;
/// 3. a `ToolApprovalRequested` event is published on the shared event bus and
///    the registered `UserInteractionHandler` is notified;
/// 4. the wait resolves when the application answers through
///    `respond_interaction` (which also completes the registry channel),
///    producing a `ToolApprovalResponseData`.
///
/// This mirrors the `USER_INTERACTION` node wait without coupling the API to a
/// live workflow: the persisted record survives a restart and the response
/// can be delivered after the requestor gave up (it then completes the
/// registry channel no-op and is recorded as responded).
/// Evaluate the approval policy for a tool call and, when a human
/// confirmation is required, open the request/response loop. Returns the
/// final approval decision.
pub async fn check_and_request_approval(
    ctx: &ApiContext,
    execution_id: &str,
    request: &ToolApprovalRequestData,
    options: Option<ToolApprovalOptions>,
    timeout_ms: u64,
) -> ApiResult<ApprovalResult> {
    let options = options.unwrap_or_else(default_approval_options);

    let engine = EngineApprovalCoordinator::new(options);
    let batch = engine.process_batch(vec![request.clone()]);
    let decision = if batch.auto_approved.contains(&0) {
        ApprovalDecision::Approve
    } else {
        ApprovalDecision::Ask
    };

    match decision {
        ApprovalDecision::Approve => Ok(ApprovalResult {
            status: ApprovalStatus::AutoApproved,
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            approved: true,
            edited_parameters: None,
            user_instruction: None,
            annotation: None,
            rejection_reason: None,
            interaction_id: None,
        }),
        ApprovalDecision::Deny(reason) => Ok(ApprovalResult {
            status: ApprovalStatus::Rejected,
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            approved: false,
            edited_parameters: None,
            user_instruction: None,
            annotation: None,
            rejection_reason: Some(reason),
            interaction_id: None,
        }),
        ApprovalDecision::Ask => {
            let (interaction_id, response) =
                request_user_approval(ctx, execution_id, request, timeout_ms).await?;
            let approved = response.approved;
            Ok(ApprovalResult {
                status: if approved {
                    ApprovalStatus::Approved
                } else {
                    ApprovalStatus::Rejected
                },
                tool_call_id: request.tool_call_id.clone(),
                tool_name: request.tool_name.clone(),
                approved,
                edited_parameters: response.edited_parameters,
                user_instruction: response.user_instruction,
                annotation: response.annotation,
                rejection_reason: response.rejection_reason,
                interaction_id: Some(interaction_id),
            })
        }
    }
}

/// Open a human approval request and wait for the response.
///
/// Persists the interaction, registers the registry channel, publishes the
/// `ToolApprovalRequested` event, notifies the registered handler, then
/// blocks until `respond_interaction` resolves it (or `timeout_ms`
/// elapses). Returns the interaction id together with the response so the
/// caller can correlate the record.
pub async fn request_user_approval(
    ctx: &ApiContext,
    execution_id: &str,
    request: &ToolApprovalRequestData,
    timeout_ms: u64,
) -> ApiResult<(String, ToolApprovalResponseData)> {
    let interaction_id = wf_common::generate_id();
    let interaction = UserInteractionStorageMetadata {
        id: interaction_id.clone(),
        execution_id: execution_id.into(),
        interaction_type: "tool_approval".into(),
        status: "pending".into(),
        request_data: serde_json::to_value(request)?,
        response_data: None,
        result_data: None,
        error: None,
        created_at: wf_common::now(),
        responded_at: None,
    };
    save_interaction(&ctx.storage, &interaction).await?;

    // Register the wait channel first so a fast response cannot race it.
    let rx = wf_workflow::interaction_registry().register(interaction_id.clone());

    // Publish the request event for server/SSE consumers.
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("interaction_id".into(), serde_json::json!(interaction_id));
    metadata.insert(
        "tool_call_id".into(),
        serde_json::json!(request.tool_call_id),
    );
    metadata.insert("tool_name".into(), serde_json::json!(request.tool_name));
    metadata.insert("request_data".into(), serde_json::to_value(request)?);
    let event = BaseEvent {
        id: wf_types::Id::new(),
        r#type: EventType::ToolApprovalRequested,
        timestamp: wf_common::now(),
        workflow_id: None,
        execution_id: Some(execution_id.to_string()),
        agent_loop_id: None,
        metadata: Some(metadata),
    };
    let _ = ctx.event_bus.publish(event);

    // Notify the registered user interaction handler.
    let request_value = serde_json::to_value(request)?;
    crate::agent::agent_user_interaction::notify_tool_approval_requested(
        ctx,
        execution_id,
        &request_value,
    )
    .await;

    let wait = tokio::time::timeout(Duration::from_millis(timeout_ms), rx).await;
    match wait {
        Ok(Ok(response_value)) => {
            let response: ToolApprovalResponseData = serde_json::from_value(response_value)
                .map_err(|e| {
                    ApiError::execution(format!(
                        "approval response for interaction {interaction_id} is malformed: {e}"
                    ))
                })?;
            Ok((interaction_id, response))
        }
        Ok(Err(_)) => Err(ApiError::execution(format!(
            "approval wait for interaction {interaction_id} was cancelled"
        ))),
        Err(_) => {
            wf_workflow::interaction_registry().remove(&interaction_id);
            Err(ApiError::Timeout(format!(
                "tool approval for '{}' timed out after {timeout_ms}ms",
                request.tool_name
            )))
        }
    }
}

/// Request approval and execute the tool when approved (TS
/// `ExecuteToolCommand` composed with the approval coordinator).
///
/// Rejects with `ApiError::Execution` when the call is denied or timed
/// out, so callers can treat the result as an execution outcome.
pub async fn execute_tool_with_approval(
    ctx: &ApiContext,
    execution_id: &str,
    tool_id: &str,
    parameters: &Value,
    options: Option<ToolExecutionOptions>,
    approval_options: Option<ToolApprovalOptions>,
    timeout_ms: u64,
) -> ApiResult<wf_types::tool::ToolExecutionResult> {
    let tool = ctx
        .tool_registry
        .get_tool(tool_id)
        .ok_or_else(|| not_found("tool", tool_id))?;
    let risk_level = tool
        .metadata
        .as_ref()
        .and_then(|m| m.risk_level)
        .map(|level| {
            serde_json::to_string(&level)
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default()
        });

    let request = ToolApprovalRequestData {
        tool_call_id: wf_common::generate_id(),
        tool_name: tool.name.clone(),
        tool_description: Some(tool.description.clone()),
        parameters: parameters.clone(),
        risk_level: risk_level.as_deref().map(ToOwned::to_owned),
        pending_queue: None,
        batch_id: None,
        tool_index: None,
        total_tools: None,
        timeout: Some(timeout_ms),
        security_preset: approval_options
            .as_ref()
            .and_then(|o| o.security_preset.as_ref())
            .map(|p| format!("{:?}", p)),
    };

    let approval =
        check_and_request_approval(ctx, execution_id, &request, approval_options, timeout_ms)
            .await?;
    if !approval.approved {
        return Err(ApiError::execution(format!(
            "tool '{}' execution rejected: {}",
            tool_id,
            approval
                .rejection_reason
                .as_deref()
                .unwrap_or("not approved")
        )));
    }

    let effective_parameters = approval
        .edited_parameters
        .clone()
        .unwrap_or_else(|| parameters.clone());
    crate::llm::tool::execute(ctx, tool_id, &effective_parameters, options, execution_id).await
}

/// Default approval options used when none are supplied: auto-approval on,
/// Balanced preset (read-only auto-approves, write/execute ask).
fn default_approval_options() -> ToolApprovalOptions {
    ToolApprovalOptions {
        auto_approval_enabled: Some(true),
        security_preset: Some(wf_types::tool::approval::SecurityPreset::Balanced),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_resource::registrar::Registries;
    use wf_resource::starter::BundleRegistry;
    use wf_storage::context::StorageContext;
    use wf_tools::executor::StatelessHandler;
    use wf_types::tool::approval::SecurityPreset;

    fn make_ctx() -> Arc<ApiContext> {
        Arc::new(ApiContext::new(
            StorageContext::new_memory(),
            Arc::new(Registries::new()),
            Arc::new(BundleRegistry::new()),
        ))
    }

    fn approval_request(
        tool_call_id: &str,
        tool_name: &str,
        risk: &str,
    ) -> ToolApprovalRequestData {
        ToolApprovalRequestData {
            tool_call_id: tool_call_id.into(),
            tool_name: tool_name.into(),
            tool_description: Some(format!("Tool {tool_name}")),
            parameters: serde_json::json!({ "command": "echo hi" }),
            risk_level: Some(risk.into()),
            pending_queue: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            timeout: None,
            security_preset: None,
        }
    }

    async fn respond(ctx: &StorageContext, interaction_id: &str, approved: bool) {
        crate::entity::user_interaction::respond_interaction(
            ctx,
            interaction_id,
            Some(serde_json::json!({ "approved": approved })),
            None,
        )
        .await
        .expect("respond should resolve");
    }

    #[tokio::test]
    async fn read_only_tool_is_auto_approved() {
        let ctx = make_ctx();
        let result = check_and_request_approval(
            &ctx,
            "exec-auto",
            &approval_request("call-1", "read_file", "read_only"),
            None,
            1000,
        )
        .await
        .unwrap();
        assert!(result.approved);
        assert!(matches!(result.status, ApprovalStatus::AutoApproved));
        assert!(result.interaction_id.is_none());
    }

    #[tokio::test]
    async fn write_tool_asks_and_responds_approved() {
        let ctx = make_ctx();

        let request = approval_request("call-2", "write_file", "write");
        let ctx_storage = ctx.storage.clone();
        let responder = tokio::spawn(async move {
            // Wait until the interaction is persisted, then answer.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let pending = crate::entity::user_interaction::list_interactions_by_status(
                ctx_storage.as_ref(),
                "pending",
            )
            .await
            .unwrap();
            assert_eq!(pending.len(), 1, "one pending approval must be persisted");
            let id = pending[0].id.to_string();
            respond(ctx_storage.as_ref(), &id, true).await;
            id
        });

        let result = check_and_request_approval(&ctx, "exec-ask", &request, None, 5000)
            .await
            .unwrap();
        assert!(result.approved);
        assert!(matches!(result.status, ApprovalStatus::Approved));
        assert!(result.interaction_id.is_some());
        assert_eq!(result.tool_name, "write_file");

        let interaction_id = responder.await.unwrap();
        assert_eq!(Some(interaction_id), result.interaction_id);

        // The interaction record is flipped to responded.
        let record = crate::entity::user_interaction::get_interaction(
            ctx.storage.as_ref(),
            result.interaction_id.as_deref().unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(record.status, "responded");
    }

    #[tokio::test]
    async fn rejection_blocks_execution() {
        let ctx = make_ctx();
        // Disable auto-approval so a safe tool still asks.
        let mut options = default_approval_options();
        options.auto_approval_enabled = Some(false);
        options.security_preset = Some(SecurityPreset::Safe);

        let request = approval_request("call-3", "write_file", "write");
        let ctx_storage = ctx.storage.clone();
        let responder = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let pending = crate::entity::user_interaction::list_interactions_by_status(
                ctx_storage.as_ref(),
                "pending",
            )
            .await
            .unwrap();
            let id = pending[0].id.to_string();
            respond(ctx_storage.as_ref(), &id, false).await;
            id
        });

        let result = check_and_request_approval(&ctx, "exec-reject", &request, Some(options), 5000)
            .await
            .unwrap();
        assert!(!result.approved);
        assert!(matches!(result.status, ApprovalStatus::Rejected));
        let _ = responder.await.unwrap();
    }

    #[tokio::test]
    async fn approval_timeout_errors() {
        let ctx = make_ctx();
        let request = approval_request("call-4", "write_file", "write");
        let err = check_and_request_approval(&ctx, "exec-timeout", &request, None, 50)
            .await
            .unwrap_err();
        assert!(matches!(err, ApiError::Timeout(_)));
    }

    fn register_echo(registry: &wf_tools::registry::ToolRegistry, tool_id: &str) {
        let handler: StatelessHandler =
            Arc::new(|params, _ctx| Ok(serde_json::json!({ "echo": params })));
        registry.register_stateless_handler(tool_id, handler);
    }

    fn echo_tool(id: &str, risk: Option<wf_types::tool::ToolRiskLevel>) -> wf_types::tool::Tool {
        wf_types::tool::Tool {
            id: id.into(),
            name: id.into(),
            description: format!("Tool {id}"),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: Some(wf_types::tool::ToolMetadata {
                category: None,
                tags: None,
                documentation_url: None,
                custom_fields: None,
                risk_level: risk,
                auto_approvable: None,
                create_checkpoint: None,
            }),
            config: None,
            enabled: Some(true),
            strict: None,
            default_timeout_ms: None,
        }
    }

    #[tokio::test]
    async fn execute_tool_with_approval_roundtrip() {
        let ctx = make_ctx();
        ctx.tool_registry.register_tool(echo_tool(
            "approval-echo",
            Some(wf_types::tool::ToolRiskLevel::Write),
        ));
        register_echo(&ctx.tool_registry, "approval-echo");

        let ctx_storage = ctx.storage.clone();
        let responder = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let pending = crate::entity::user_interaction::list_interactions_by_status(
                ctx_storage.as_ref(),
                "pending",
            )
            .await
            .unwrap();
            let id = pending[0].id.to_string();
            respond(ctx_storage.as_ref(), &id, true).await;
        });

        let result = execute_tool_with_approval(
            &ctx,
            "exec-approval",
            "approval-echo",
            &serde_json::json!({ "x": 1 }),
            None,
            None,
            5000,
        )
        .await
        .unwrap();
        assert!(result.success);
        assert_eq!(
            result.result,
            Some(serde_json::json!({ "echo": { "x": 1 } }))
        );
        responder.await.unwrap();
    }
}
