use std::sync::Arc;

use wf_tools::executor::stateless::StatelessAsyncHandler;
use wf_tools::registry::ToolRegistry;
use wf_types::tool::{Tool, ToolPropertySchema, ToolType};

use wf_checkpoint::file::FileCheckpointManager;

const APPROVE_CHANGES_TOOL_ID: &str = "approve_changes";
const DEFAULT_FEATURE: &str = "default";

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
                      into its feature; rejecting discards them."
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
                        description: Some("Human-readable reason for the decision".to_string()),
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
            let reason = args
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let outcome = if approve {
                manager.approve_pending(agent_instance_id, DEFAULT_FEATURE)
            } else {
                manager.reject_changes(agent_instance_id).map(|baseline| {
                    wf_checkpoint::approval::MergeOutcome {
                        merged: false,
                        snapshot_id: baseline,
                        conflicts: vec![],
                        conflict_files: vec![],
                        message: format!("changes rejected (reason: {reason})"),
                    }
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
