use std::sync::Arc;

use wf_tools::executor::stateless::StatelessAsyncHandler;
use wf_tools::registry::ToolRegistry;
use wf_types::tool::{Tool, ToolPropertySchema, ToolType};

use wf_checkpoint::file::FileCheckpointManager;

const APPROVE_CHANGES_TOOL_ID: &str = "approve_changes";
const DEFAULT_FEATURE: &str = "default";

/// Minimum trimmed length of a rejection reason (characters).
const MIN_REJECT_REASON_LEN: usize = 8;
/// Maximum rejection reason length (characters); longer reasons are
/// truncated and the outcome message notes the truncation.
const MAX_REJECT_REASON_LEN: usize = 2000;

/// Register the `approve_changes` tool (approval policy `llm` path): an
/// in-workflow tool an LLM node can call to approve or reject the pending
/// file changes of an agent execution. Approving merges the actor's approval
/// partition into its feature partition; rejecting rolls the partition back
/// to its baseline. The tool is only registered when a file checkpoint
/// manager is attached.
pub fn register_approval_tools(registry: &ToolRegistry, manager: FileCheckpointManager) {
    registry.register_tool(Tool {
        id: wf_types::Id::from(APPROVE_CHANGES_TOOL_ID),
        name: APPROVE_CHANGES_TOOL_ID.to_string(),
        description: "Approve or reject the pending file changes of an agent execution \
                      (approval policy `llm` / `manual`). Approving merges the agent's changes \
                      into its feature; rejecting discards them. A rejection must carry a \
                      specific actionable reason (which check failed, what is unacceptable, \
                      what to change next); approvals may omit the reason."
            .to_string(),
        tool_type: ToolType::Stateless,
        parameters: Some(wf_types::tool::ToolParameterSchema {
            r#type: "object".to_string(),
            properties: std::collections::BTreeMap::from([
                (
                    "agent_instance_id".to_string(),
                    ToolPropertySchema {
                        description: Some(
                            "Execution (entity) id of the agent whose changes are pending"
                                .to_string(),
                        ),
                        ..ToolPropertySchema::typed("string")
                    },
                ),
                (
                    "approve".to_string(),
                    ToolPropertySchema {
                        description: Some(
                            "true merges the changes, false rejects them".to_string(),
                        ),
                        ..ToolPropertySchema::typed("boolean")
                    },
                ),
                (
                    "reason".to_string(),
                    ToolPropertySchema {
                        description: Some(
                            "Human-readable reason for the decision. Required when `approve` \
                             is false (at least 8 non-blank characters); optional otherwise."
                                .to_string(),
                        ),
                        ..ToolPropertySchema::typed("string")
                    },
                ),
            ]),
            required: vec!["agent_instance_id".to_string(), "approve".to_string()],
            additional_properties: Some(false),
        }),
        metadata: None,
        config: None,
        enabled: Some(true),
        strict: Some(true),
        default_timeout_ms: None,
    });

    let handler: StatelessAsyncHandler = Arc::new(move |args, _ctx| {
        let manager = manager.clone();
        Box::pin(async move {
            let agent_instance_id = args
                .get("agent_instance_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    wf_tools::error::ToolError::ValidationFailed(
                        "agent_instance_id is required".to_string(),
                    )
                })?;
            let approve = args
                .get("approve")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| {
                    wf_tools::error::ToolError::ValidationFailed(
                        "approve (boolean) is required".to_string(),
                    )
                })?;
            let raw_reason = args.get("reason").and_then(|v| v.as_str());

            let outcome = if approve {
                manager.approve_pending(agent_instance_id, DEFAULT_FEATURE)
            } else {
                let trimmed = raw_reason.map(str::trim).unwrap_or("");
                if trimmed.chars().count() < MIN_REJECT_REASON_LEN {
                    return Err(wf_tools::error::ToolError::ValidationFailed(format!(
                        "reason is required when `approve` is false: explain which check \
                         failed and what to change next (at least {MIN_REJECT_REASON_LEN} \
                         non-blank characters)"
                    )));
                }
                let (reason, truncated) = if trimmed.chars().count() > MAX_REJECT_REASON_LEN {
                    (
                        trimmed
                            .chars()
                            .take(MAX_REJECT_REASON_LEN)
                            .collect::<String>(),
                        true,
                    )
                } else {
                    (trimmed.to_string(), false)
                };
                manager
                    .reject_changes(agent_instance_id, Some(&reason))
                    .map(|baseline| wf_checkpoint::approval::MergeOutcome {
                        merged: false,
                        snapshot_id: baseline,
                        conflicts: vec![],
                        conflict_files: vec![],
                        message: if truncated {
                            format!(
                                "changes rejected (reason: {reason}... \
                                     [truncated to {MAX_REJECT_REASON_LEN} chars])"
                            )
                        } else {
                            format!("changes rejected (reason: {reason})")
                        },
                    })
            };
            match outcome {
                Ok(outcome) => serde_json::to_value(outcome).map_err(|e| {
                    wf_tools::error::ToolError::ExecutionFailed {
                        tool_id: APPROVE_CHANGES_TOOL_ID.to_string(),
                        reason: format!("failed to serialize merge outcome: {e}"),
                    }
                }),
                Err(err) => Err(wf_tools::error::ToolError::ExecutionFailed {
                    tool_id: APPROVE_CHANGES_TOOL_ID.to_string(),
                    reason: err.to_string(),
                }),
            }
        })
    });
    registry.register_stateless_async_handler(APPROVE_CHANGES_TOOL_ID, handler);
}

#[cfg(test)]
mod tests {
    use super::*;
    use wf_checkpoint::file::FileContentEntry;
    use wf_tools::executor::trait_def::ToolExecutionContext;
    use wf_types::tool::ToolExecutionOptions;

    fn test_manager() -> FileCheckpointManager {
        FileCheckpointManager::new_in_memory().unwrap()
    }

    fn pending_manager() -> FileCheckpointManager {
        let manager = test_manager();
        manager
            .create_checkpoint("e1", &[FileContentEntry::new("a.txt", b"edit".to_vec())])
            .unwrap();
        manager.move_agent_to_approval("e1").unwrap();
        manager
    }

    fn test_registry(manager: FileCheckpointManager) -> ToolRegistry {
        let registry = ToolRegistry::new();
        register_approval_tools(&registry, manager);
        registry
    }

    fn options() -> ToolExecutionOptions {
        ToolExecutionOptions {
            timeout: Some(30000),
            retries: None,
            retry_delay: None,
            exponential_backoff: None,
        }
    }

    fn context() -> ToolExecutionContext {
        ToolExecutionContext::new("test-approval".to_string())
    }

    async fn call(
        registry: &ToolRegistry,
        approve: bool,
        reason: Option<serde_json::Value>,
    ) -> Result<wf_types::tool::ToolExecutionResult, wf_tools::error::ToolError> {
        let mut args = serde_json::json!({
            "agent_instance_id": "e1",
            "approve": approve,
        });
        if let Some(reason) = reason {
            args["reason"] = reason;
        }
        registry
            .execute_tool(APPROVE_CHANGES_TOOL_ID, &args, &options(), &context())
            .await
    }

    #[tokio::test]
    async fn reject_without_reason_is_rejected() {
        let manager = pending_manager();
        let registry = test_registry(manager.clone());
        // The executor surfaces handler validation failures as an
        // unsuccessful result, not as a call error; either way the pending
        // approval is untouched.
        let outcome = call(&registry, false, None).await.unwrap();
        assert!(!outcome.success);
        let error = outcome.error.unwrap();
        assert!(error.contains("reason is required"), "error: {error}");
        assert_eq!(manager.list_pending_approvals().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reject_with_blank_or_short_reason_is_rejected() {
        let registry = test_registry(pending_manager());
        for reason in ["", "   ", "no", "too bad"] {
            let outcome = call(&registry, false, Some(serde_json::json!(reason)))
                .await
                .unwrap();
            assert!(!outcome.success, "reason {reason:?} must not succeed");
            let error = outcome.error.unwrap();
            assert!(
                error.contains("reason is required"),
                "reason {reason:?}: {error}"
            );
        }
    }

    #[tokio::test]
    async fn reject_with_valid_reason_rolls_back_and_reports_it() {
        let manager = pending_manager();
        let registry = test_registry(manager.clone());
        let reason = "the edit drops error handling; restore it and resubmit";
        let outcome = call(&registry, false, Some(serde_json::json!(reason)))
            .await
            .unwrap();
        assert!(outcome.success);
        let message = outcome.result.unwrap()["message"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(message.contains(reason), "message: {message}");
        assert!(manager.list_pending_approvals().unwrap().is_empty());
    }

    #[tokio::test]
    async fn overlong_reason_is_truncated_and_noted() {
        let registry = test_registry(pending_manager());
        let reason = "x".repeat(MAX_REJECT_REASON_LEN + 100);
        let outcome = call(&registry, false, Some(serde_json::json!(reason)))
            .await
            .unwrap();
        assert!(outcome.success);
        let message = outcome.result.unwrap()["message"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(message.contains("[truncated"), "message: {message}");
    }

    #[tokio::test]
    async fn approve_without_reason_still_merges() {
        let registry = test_registry(pending_manager());
        let outcome = call(&registry, true, None).await.unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.result.unwrap()["merged"], serde_json::json!(true));
    }
}
