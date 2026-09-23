use wf_checkpoint::approval::{MergeOutcome, PendingApproval};
use wf_types::llm::LlmRequest;
use wf_types::message::{Message, MessageContentValue, MessageRole};

use crate::infra::context::ApiContext;
use crate::ApiError;
use crate::ApiResult;

/// The only verdict tool a single-shot approval reviewer may call.
pub const REVIEW_VERDICT_TOOL: &str = "approve_changes";

/// The attached file checkpoint manager, or an error when file
/// checkpointing is disabled.
fn manager(ctx: &ApiContext) -> ApiResult<&wf_checkpoint::file::FileCheckpointManager> {
    ctx.file_checkpoint_manager().ok_or_else(|| {
        ApiError::execution("file checkpointing is not enabled; set file_checkpoint.enabled=true")
    })
}

/// All pending layered approvals: actor partitions at the approval layer
/// submitted but neither merged nor rejected. Persisted in Sqlite, so the
/// list survives across executions ("review after the run ends").
pub fn list_pending_approvals(ctx: &ApiContext) -> ApiResult<Vec<PendingApproval>> {
    manager(ctx)?
        .list_pending_approvals()
        .map_err(ApiError::execution_with_source)
}

/// Approve a pending approval: merge the actor's changes into the named
/// feature partition under the configured conflict behavior. `paths`
/// selects file-level approval — when `Some` and non-empty, only the listed
/// files are advanced into the feature and the rest stay pending in the
/// approval layer. Returns the merge outcome (conflicts, marker files,
/// snapshot id).
pub fn approve_changes(
    ctx: &ApiContext,
    agent_instance_id: &str,
    feature_name: &str,
    paths: Option<Vec<String>>,
) -> ApiResult<MergeOutcome> {
    let manager = manager(ctx)?;
    let feature = if feature_name.is_empty() {
        wf_checkpoint::file::FileCheckpointManager::default_feature_name(agent_instance_id)
    } else {
        feature_name.to_string()
    };
    let outcome = match paths {
        Some(paths) if !paths.is_empty() => {
            manager.approve_pending_paths(agent_instance_id, &feature, paths)
        }
        _ => manager.approve_pending(agent_instance_id, &feature),
    };
    outcome.map_err(ApiError::execution_with_source)
}

/// Reject a pending approval: roll the actor's approval partition back to
/// its baseline. `reason` is optional for human callers and is only used
/// for logging and diagnostics. Returns the baseline snapshot id (hex).
pub fn reject_changes(
    ctx: &ApiContext,
    agent_instance_id: &str,
    reason: Option<&str>,
) -> ApiResult<String> {
    manager(ctx)?
        .reject_changes(agent_instance_id, reason)
        .map_err(ApiError::execution_with_source)
}

/// Outcome of a single-shot approval review.
///
/// `Decided` carries the merge outcome of the executed verdict tool.
/// `Unresolved` means the reviewer could not decide (no pending approval,
/// model failure, invalid verdict call even after one retry): the caller
/// must leave the changes pending for manual review, never auto-merge or
/// auto-reject them.
#[derive(Debug, Clone)]
pub enum ReviewOutcome {
    Decided(MergeOutcome),
    Unresolved(String),
}

fn review_message(role: MessageRole, text: String) -> Message {
    Message {
        id: wf_types::Id::new(),
        role,
        content: MessageContentValue::Text(text),
        timestamp: wf_common::now(),
        tool_call_id: None,
        tool_name: None,
        tool_calls: None,
        thinking: None,
        metadata: None,
    }
}

fn review_prompt(
    agent_instance_id: &str,
    feature: &str,
    pending: &PendingApproval,
) -> Vec<Message> {
    let mut changes = String::new();
    for change in &pending.changes {
        changes.push_str(&format!(
            "- file: {} (source: {}, hash: {})",
            change.file, change.source, change.hash
        ));
        if let Some(message) = change.message.as_deref() {
            changes.push_str(&format!(" intent: {message}"));
        }
        changes.push('\n');
    }
    if changes.is_empty() {
        changes.push_str("(no per-file change summaries recorded)\n");
    }
    let system = "You review pending file changes of an agent execution. Decide once: \
        call `approve_changes` with `approve: true` to merge them, or `approve: false` \
        to discard them. A rejection must carry a specific actionable reason (which \
        check failed, what is unacceptable, what to change next). Call exactly one \
        tool and no other tool."
        .to_string();
    let user = format!(
        "Agent execution id: {agent_instance_id}\nTarget feature: {feature}\n\
         Submitted changes:\n{changes}\n\
         Pass `agent_instance_id` through unchanged in your tool call."
    );
    vec![
        review_message(MessageRole::System, system),
        review_message(MessageRole::User, user),
    ]
}

async fn run_review_round(
    ctx: &ApiContext,
    request: &LlmRequest,
    review_execution_id: &str,
) -> ApiResult<ReviewOutcome> {
    let allowed = vec![REVIEW_VERDICT_TOOL.to_string()];
    let outcome =
        crate::llm::generate_with_tools_once(ctx, request, &allowed, None, review_execution_id)
            .await?;
    if outcome.executions.len() != 1 {
        return Err(ApiError::execution(format!(
            "approval review must emit exactly one verdict call, got {}",
            outcome.executions.len()
        )));
    }
    let execution = &outcome.executions[0];
    if !execution.result.success {
        return Err(ApiError::execution(format!(
            "verdict tool '{}' failed: {}",
            execution.tool_name,
            execution.result.error.as_deref().unwrap_or("unknown error")
        )));
    }
    match execution.result.result.clone() {
        Some(value) => match serde_json::from_value::<MergeOutcome>(value) {
            Ok(merge) => Ok(ReviewOutcome::Decided(merge)),
            Err(e) => Err(ApiError::execution(format!(
                "verdict tool '{}' returned an unreadable outcome: {e}",
                execution.tool_name
            ))),
        },
        None => Err(ApiError::execution(format!(
            "verdict tool '{}' returned no outcome",
            execution.tool_name
        ))),
    }
}

/// Review a pending approval with a single-shot model call.
///
/// Builds the review prompt from the pending change summaries and runs the
/// shared single-shot primitive with only the verdict tool visible. One
/// bounded retry is allowed when the first round fails, with the failure
/// fed back to the model; anything beyond that resolves to `Unresolved`
/// so the caller leaves the changes pending for manual review.
pub async fn review_pending_approval(
    ctx: &ApiContext,
    agent_instance_id: &str,
    feature_name: &str,
    profile_id: &str,
) -> ApiResult<ReviewOutcome> {
    let manager = manager(ctx)?;
    let expected_actor = manager.actor_id_for(agent_instance_id).as_str().to_string();
    let pending = manager
        .list_pending_approvals()
        .map_err(ApiError::execution_with_source)?;
    let Some(view) = pending.iter().find(|p| p.actor == expected_actor) else {
        return Ok(ReviewOutcome::Unresolved(format!(
            "no pending approval for '{agent_instance_id}'"
        )));
    };
    let Some(tool) = ctx.tool_registry.get_tool(REVIEW_VERDICT_TOOL) else {
        return Ok(ReviewOutcome::Unresolved(format!(
            "verdict tool '{REVIEW_VERDICT_TOOL}' is not registered"
        )));
    };
    let feature = if feature_name.is_empty() {
        wf_checkpoint::file::FileCheckpointManager::default_feature_name(agent_instance_id)
    } else {
        feature_name.to_string()
    };
    let review_execution_id = format!("approval-review-{agent_instance_id}");
    let messages = review_prompt(agent_instance_id, &feature, view);
    let request = LlmRequest {
        profile_id: profile_id.to_string(),
        messages,
        parameters: None,
        generation: None,
        tools: Some(vec![tool]),
        tool_call_protocol: None,
        locked_tool_call_protocol: None,
        violation_policy: None,
        execution_id: Some(review_execution_id.clone()),
        stream: None,
        dead_loop_detection: None,
        protocol_auto_converted: None,
    };

    match run_review_round(ctx, &request, &review_execution_id).await {
        Ok(outcome) => Ok(outcome),
        Err(first) => {
            tracing::warn!(
                entity = %agent_instance_id,
                error = %first,
                "approval review round failed, retrying once with the failure fed back"
            );
            let mut messages = request.messages.clone();
            messages.push(review_message(
                MessageRole::User,
                format!(
                    "Your previous verdict call failed: {first}. Fix the call \
                     (pass a valid `agent_instance_id`, a boolean `approve`, and a \
                     specific reason when rejecting) and retry."
                ),
            ));
            let retry = LlmRequest {
                messages,
                ..request
            };
            match run_review_round(ctx, &retry, &review_execution_id).await {
                Ok(outcome) => Ok(outcome),
                Err(second) => {
                    tracing::warn!(
                        entity = %agent_instance_id,
                        error = %second,
                        "approval review retry failed, leaving changes pending for manual review"
                    );
                    Ok(ReviewOutcome::Unresolved(format!(
                        "review failed after one retry: {second}"
                    )))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use wf_checkpoint::file::{FileCheckpointManager, FileContentEntry};
    use wf_llm::{LlmResponseSpec, MockLlmClient};
    use wf_resource::registry::ResourceRegistries;
    use wf_storage::context::StorageContext;
    use wf_types::llm::LlmFormat;
    use wf_types::message::{LlmFunctionCall, LlmToolCall};

    fn make_ctx(manager: FileCheckpointManager) -> (Arc<ApiContext>, Arc<MockLlmClient>) {
        let ctx = Arc::new(
            ApiContext::new(
                StorageContext::new_memory(),
                Arc::new(ResourceRegistries::new()),
            )
            .with_file_checkpoint_manager(manager),
        );
        ctx.tool_registry.register_tool(wf_types::tool::Tool {
            id: wf_types::Id::from(REVIEW_VERDICT_TOOL),
            name: REVIEW_VERDICT_TOOL.to_string(),
            description: "Verdict stub".to_string(),
            tool_type: wf_types::tool::ToolType::Stateless,
            parameters: None,
            metadata: None,
            config: None,
            enabled: Some(true),
            strict: Some(true),
            default_timeout_ms: None,
        });
        let handler: wf_tools::executor::stateless::StatelessAsyncHandler =
            Arc::new(move |args, _ctx| {
                Box::pin(async move {
                    let approve = args
                        .get("approve")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    let reason = args
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let outcome = MergeOutcome {
                        merged: approve,
                        snapshot_id: "baseline".to_string(),
                        conflicts: vec![],
                        conflict_files: vec![],
                        message: format!("stub verdict approve={approve} (reason: {reason})"),
                    };
                    serde_json::to_value(outcome).map_err(|e| {
                        wf_tools::error::ToolError::ExecutionFailed {
                            tool_id: REVIEW_VERDICT_TOOL.to_string(),
                            reason: format!("stub serialization failed: {e}"),
                        }
                    })
                })
            });
        ctx.tool_registry
            .register_stateless_async_handler(REVIEW_VERDICT_TOOL, handler);
        let mock = Arc::new(MockLlmClient::new());
        ctx.llm_gateway.register_mock("mock-review", mock.clone());
        ctx.llm_gateway
            .register_profile(wf_types::llm::LlmProfile {
                id: "mock-review".to_string(),
                name: "mock-review".to_string(),
                format: LlmFormat::OpenaiChat,
                provider_id: None,
                model: "mock-model".to_string(),
                api_key: Some("sk-test".into()),
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
            })
            .expect("mock profile registers");
        (ctx, mock)
    }

    fn pending_manager(entity: &str) -> FileCheckpointManager {
        let manager = FileCheckpointManager::new_in_memory().expect("in-memory manager");
        manager
            .create_checkpoint(entity, &[FileContentEntry::new("a.txt", b"edit".to_vec())])
            .expect("checkpoint");
        manager
            .move_agent_to_approval(entity)
            .expect("move to approval");
        manager
    }

    fn verdict_call(arguments: serde_json::Value) -> LlmToolCall {
        LlmToolCall {
            id: "call-1".to_string(),
            r#type: "function".to_string(),
            function: LlmFunctionCall {
                name: REVIEW_VERDICT_TOOL.to_string(),
                arguments: arguments.to_string(),
            },
        }
    }

    #[tokio::test]
    async fn review_executes_model_verdict_and_decides() {
        let (ctx, mock) = make_ctx(pending_manager("e1"));
        mock.script(LlmResponseSpec::tool_calls(vec![verdict_call(
            serde_json::json!({
                "agent_instance_id": "e1",
                "approve": false,
                "reason": "the edit drops error handling; restore it and resubmit",
            }),
        )]));

        match review_pending_approval(&ctx, "e1", "", "mock-review")
            .await
            .expect("review runs")
        {
            ReviewOutcome::Decided(merge) => {
                assert!(!merge.merged);
                assert!(merge.message.contains("drops error handling"));
            }
            ReviewOutcome::Unresolved(reason) => panic!("expected a decision, got {reason}"),
        }
        assert_eq!(mock.recorded_count(), 1);
    }

    #[tokio::test]
    async fn review_without_pending_approval_is_unresolved_without_a_model_call() {
        let (ctx, mock) = make_ctx(pending_manager("e1"));
        match review_pending_approval(&ctx, "ghost", "", "mock-review")
            .await
            .expect("review runs")
        {
            ReviewOutcome::Unresolved(reason) => assert!(reason.contains("no pending approval")),
            ReviewOutcome::Decided(_) => panic!("expected unresolved without pending approval"),
        }
        assert_eq!(mock.recorded_count(), 0);
    }

    #[tokio::test]
    async fn review_with_text_only_model_is_unresolved_after_one_retry() {
        let (ctx, mock) = make_ctx(pending_manager("e1"));
        mock.script(LlmResponseSpec::text("looks fine, no tool call"));
        mock.script(LlmResponseSpec::text("still no tool call"));
        match review_pending_approval(&ctx, "e1", "", "mock-review")
            .await
            .expect("review runs")
        {
            ReviewOutcome::Unresolved(_) => {}
            ReviewOutcome::Decided(_) => panic!("expected unresolved for text-only model"),
        }
        assert_eq!(mock.recorded_count(), 2);
    }
}
