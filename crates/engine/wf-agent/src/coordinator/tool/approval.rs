use serde_json::Value;
use std::sync::Arc;

use wf_tools::approval::{ApprovalDecision, ToolApprovalCoordinator};
use wf_types::interaction::tool_approval::{PendingToolCallInfo, ToolApprovalRequestData};
use wf_types::message::LlmToolCall;
use wf_types::tool::approval::ToolApprovalOptions;
use wf_types::tool::file_permission::FilePermissionSettings;

use crate::approval::{ToolApprovalHandler, ToolApprovalRequest};
use crate::entity::AgentLoopEntity;

use super::runner::risk_level_of;
use super::types::ApprovalOutcome;

/// Approval engine adapter for a batch of tool calls.
///
/// Wraps the policy evaluation (`ToolApprovalCoordinator`) plus the optional
/// human handler: policy denials are final and never escalate; pending calls
/// are routed through the registered handler with the batch context.
pub(crate) struct ToolApprovalGate {
    options: Option<ToolApprovalOptions>,
    handler: Option<Arc<dyn ToolApprovalHandler>>,
}

impl ToolApprovalGate {
    pub(crate) fn new(
        options: Option<ToolApprovalOptions>,
        handler: Option<Arc<dyn ToolApprovalHandler>>,
    ) -> Self {
        Self { options, handler }
    }

    /// Current approval wiring (options + handler); lets callers rebuild
    /// the coordinator without silently dropping the approval contract.
    pub(crate) fn config(
        &self,
    ) -> (
        Option<ToolApprovalOptions>,
        Option<Arc<dyn ToolApprovalHandler>>,
    ) {
        (self.options.clone(), self.handler.clone())
    }

    /// Run the approval engine for a batch of tool calls. Produces one
    /// outcome per tool call, in order.
    pub(crate) async fn approve_tool_calls(
        &self,
        entity: &AgentLoopEntity,
        tool_calls: &[LlmToolCall],
        registry: &wf_tools::registry::ToolRegistry,
    ) -> Vec<ApprovalOutcome> {
        // Fast path: no handler and no options -> auto-approve everything.
        if self.handler.is_none() && self.options.is_none() {
            return tool_calls
                .iter()
                .map(|_| ApprovalOutcome::Execute {
                    edited_parameters: None,
                })
                .collect();
        }

        let requests: Vec<ToolApprovalRequestData> = tool_calls
            .iter()
            .map(|tc| ToolApprovalRequestData {
                tool_call_id: tc.id.clone(),
                tool_name: tc.function.name.clone(),
                tool_description: None,
                parameters: serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Null),
                risk_level: risk_level_of(registry, &tc.function.name),
                pending_queue: None,
                batch_id: None,
                tool_index: None,
                total_tools: None,
                timeout: None,
                security_preset: None,
            })
            .collect();

        // When a handler is registered it controls the policy; without
        // explicit options fall back to ask-everything for the handler.
        let options = self.options.clone().unwrap_or_else(|| ToolApprovalOptions {
            auto_approval_enabled: Some(self.handler.is_none()),
            security_preset: None,
            risk_threshold: None,
            auto_approve_patterns: None,
            categories: None,
            workspace_boundary: None,
            file_permissions: Some(FilePermissionSettings::default_rules()),
            command: None,
            mcp: None,
            network: None,
            interaction: None,
            allow_write_protected: None,
        });

        let coordinator = ToolApprovalCoordinator::new(options);
        let decisions = coordinator.evaluate(&requests);
        let batch = coordinator.process_batch(requests);

        // Policy denials are final: they must never be escalated into a
        // human approval request, so drop them from the pending set before
        // the interaction loop runs.
        let asks: Vec<usize> = batch
            .pending
            .iter()
            .copied()
            .filter(|idx| !matches!(decisions[*idx], ApprovalDecision::Deny(_)))
            .collect();

        let mut outcomes: Vec<ApprovalOutcome> = decisions
            .iter()
            .enumerate()
            .map(|(idx, decision)| match decision {
                ApprovalDecision::Deny(reason) => ApprovalOutcome::Rejected {
                    reason: reason.clone(),
                },
                _ => ApprovalOutcome::Rejected {
                    reason: format!("internal: unclassified (tool call {idx})"),
                },
            })
            .collect();

        for idx in &batch.auto_approved {
            outcomes[*idx] = ApprovalOutcome::Execute {
                edited_parameters: None,
            };
        }

        for idx in &asks {
            let tc = &tool_calls[*idx];
            let outcome = match self.handler.as_ref() {
                Some(handler) => {
                    let interaction_id = format!("approval-{}-{}", wf_common::now(), tc.id);
                    let request = ToolApprovalRequest {
                        tool_call_id: tc.id.clone(),
                        tool_name: tc.function.name.clone(),
                        arguments: serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(Value::Null),
                        interaction_id,
                        batch_id: Some(batch.batch_id.clone()),
                        tool_index: Some(*idx as u32),
                        total_tools: Some(tool_calls.len() as u32),
                        pending_queue: Some(
                            asks.iter()
                                .map(|p| PendingToolCallInfo {
                                    id: tool_calls[*p].id.clone(),
                                    name: tool_calls[*p].function.name.clone(),
                                    arguments: Some(
                                        serde_json::from_str(&tool_calls[*p].function.arguments)
                                            .unwrap_or(Value::Null),
                                    ),
                                    risk_level: None,
                                })
                                .collect(),
                        ),
                    };

                    // Approval waits must not consume the wall-clock budget.
                    let _guard = entity.timeout_manager().pause_handle();
                    let result = handler.request_approval(&request).await;
                    if result.approved {
                        ApprovalOutcome::Execute {
                            edited_parameters: result.edited_parameters,
                        }
                    } else {
                        ApprovalOutcome::Rejected {
                            reason: result
                                .rejection_reason
                                .unwrap_or_else(|| "Rejected by user".to_string()),
                        }
                    }
                }
                None => ApprovalOutcome::Rejected {
                    reason: format!(
                        "No approval handler configured. Tool \"{}\" requires manual approval but no handler is registered.",
                        tc.function.name
                    ),
                },
            };
            outcomes[*idx] = outcome;
        }

        outcomes
    }
}
