//! Tool approval coordination: policy evaluation, batching and results.
//!
//! The coordinator decides per tool call whether it can run automatically
//! ([`ApprovalDecision::Approve`]), must be denied, or has to be escalated to
//! a human ([`ApprovalDecision::Ask`]). The concrete policies live in the
//! [`file_policy`], [`network_policy`] and [`mcp`] submodules.

pub mod file_policy;
pub mod mcp;
pub mod network_policy;

#[cfg(test)]
mod tests;

pub use file_policy::check_file_permission;
pub use mcp::{check_mcp_approval, McpToolContext};

use std::collections::HashMap;

use wf_types::interaction::tool_approval::{ToolApprovalRequestData, ToolApprovalResponseData};
use wf_types::tool::approval::{
    CommandApprovalSettings, SecurityPreset, ToolApprovalOptions, ToolApprovalResult,
};
use wf_types::tool::file_permission::FileOperationType;
use wf_types::tool::{Tool, ToolRiskLevel};

use wf_shell::command_safety::{CommandDecision, CommandPolicy};
use wf_shell::config::ShellToolConfig;

use file_policy::{extract_file_operation, extract_file_paths};
use mcp::handle_mcp_approval;
use network_policy::{domain_matches, extract_domain};

#[derive(Debug, Clone)]
pub struct ToolBatch {
    pub batch_id: String,
    pub tools: Vec<ToolApprovalRequestData>,
    pub auto_approved: Vec<usize>,
    pub pending: Vec<usize>,
    pub results: HashMap<String, ToolApprovalResponseData>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApprovalDecision {
    Approve,
    Deny(String),
    Ask,
}

pub struct ToolApprovalCoordinator {
    options: ToolApprovalOptions,
    protect_controller: Option<crate::protect::ProtectController>,
}

impl ToolApprovalCoordinator {
    pub fn new(options: ToolApprovalOptions) -> Self {
        Self {
            options,
            protect_controller: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self {
            options: ToolApprovalOptions::balanced_defaults(),
            protect_controller: None,
        }
    }

    pub fn with_protect_controller(mut self, pc: crate::protect::ProtectController) -> Self {
        self.protect_controller = Some(pc);
        self
    }

    pub fn process_batch(&self, tools: Vec<ToolApprovalRequestData>) -> ToolBatch {
        let decisions = self.evaluate(&tools);
        Self::build_batch(tools, &decisions)
    }

    /// Evaluate with per-request registry context (see
    /// [`McpToolContext`]). The context slice must align with `tools`;
    /// a shorter slice degrades the tail to parameter-only evaluation.
    pub fn evaluate_with_mcp_context(
        &self,
        tools: &[ToolApprovalRequestData],
        contexts: &[McpToolContext<'_>],
    ) -> Vec<ApprovalDecision> {
        tools
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                let ctx = contexts.get(idx);
                self.check_approval(
                    t,
                    ctx.and_then(|c| c.tool),
                    ctx.and_then(|c| c.mcp_registry),
                )
            })
            .collect()
    }

    pub fn process_batch_with_mcp_context(
        &self,
        tools: Vec<ToolApprovalRequestData>,
        contexts: &[McpToolContext<'_>],
    ) -> ToolBatch {
        let decisions = self.evaluate_with_mcp_context(&tools, contexts);
        Self::batch_from_decisions(tools, &decisions)
    }

    /// Build a batch from precomputed decisions, avoiding a second
    /// evaluation pass when the caller already has them.
    pub fn batch_from_decisions(
        tools: Vec<ToolApprovalRequestData>,
        decisions: &[ApprovalDecision],
    ) -> ToolBatch {
        Self::build_batch(tools, decisions)
    }

    fn build_batch(
        tools: Vec<ToolApprovalRequestData>,
        decisions: &[ApprovalDecision],
    ) -> ToolBatch {
        let batch_id = wf_common::generate_id();
        let mut auto_approved = Vec::new();
        let mut pending = Vec::new();

        for (idx, decision) in decisions.iter().enumerate() {
            match decision {
                ApprovalDecision::Approve => {
                    auto_approved.push(idx);
                }
                _ => {
                    pending.push(idx);
                }
            }
        }

        ToolBatch {
            batch_id,
            tools,
            auto_approved,
            pending,
            results: HashMap::new(),
        }
    }

    /// Per-tool policy decisions, in input order. Unlike
    /// [`Self::process_batch`] this keeps explicit policy denials distinct
    /// from human-approval requests, so callers can finalize denied calls
    /// without escalating them to an interaction.
    pub fn evaluate(&self, tools: &[ToolApprovalRequestData]) -> Vec<ApprovalDecision> {
        tools
            .iter()
            .map(|t| self.check_approval(t, None, None))
            .collect()
    }

    fn check_approval(
        &self,
        tool: &ToolApprovalRequestData,
        mcp_tool: Option<&Tool>,
        mcp_registry: Option<&crate::mcp::connection::McpServerRegistry>,
    ) -> ApprovalDecision {
        if self.options.auto_approval_enabled != Some(true) {
            return ApprovalDecision::Ask;
        }

        if let Some(ref patterns) = self.options.auto_approve_patterns {
            if !patterns.is_empty() {
                let matched = patterns.iter().any(|p| {
                    if let Ok(glob) = globset::GlobBuilder::new(p).case_insensitive(true).build() {
                        glob.compile_matcher().is_match(&tool.tool_name)
                    } else {
                        false
                    }
                });
                if matched {
                    return ApprovalDecision::Approve;
                }
            }
        }

        let risk_level =
            ToolRiskLevel::parse_case_insensitive(tool.risk_level.as_deref().unwrap_or("write"));

        if let Some(ref fp) = self.options.file_permissions {
            if let Ok(op) = extract_file_operation(&tool.tool_name, risk_level, &tool.parameters) {
                let paths = extract_file_paths(&tool.parameters);
                if !paths.is_empty() && paths.iter().any(|p| !check_file_permission(p, &op, fp)) {
                    // Sensitive or protected files (e.g. .env, *.pem,
                    // secrets/**) must be routed through human review rather
                    // than hard-denied: editing them is often legitimate
                    // (the user asked the agent to change .env), it just
                    // needs explicit sign-off. Reserve the hard `Deny` for
                    // write-protected files guarded by the protect_controller
                    // below, which are never meant to be edited at all.
                    return ApprovalDecision::Ask;
                }
            }
        }

        if let Some(ref pc) = self.protect_controller {
            if let Ok(op) = extract_file_operation(&tool.tool_name, risk_level, &tool.parameters) {
                if (op == FileOperationType::Write || op == FileOperationType::Delete)
                    && self.options.allow_write_protected != Some(true)
                    && extract_file_paths(&tool.parameters)
                        .iter()
                        .any(|p| pc.is_write_protected(p))
                {
                    return ApprovalDecision::Deny("File is write-protected".to_string());
                }
            }
        }

        match risk_level {
            Some(ToolRiskLevel::ReadOnly) => {
                let cat = self.options.categories.as_ref();
                let allow = cat.and_then(|c| c.always_allow_read_only).unwrap_or(true);
                if allow {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            Some(ToolRiskLevel::Write) => {
                let cat_allow = self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_write)
                    .unwrap_or(false);
                if cat_allow || self.options.security_preset == Some(SecurityPreset::Permissive) {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            Some(ToolRiskLevel::Execute) => {
                let cat_allow = self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_execute)
                    .unwrap_or(false);
                if cat_allow {
                    handle_execute_command_approval(&self.options, tool)
                } else if self.options.security_preset == Some(SecurityPreset::Permissive) {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            Some(ToolRiskLevel::Mcp) => {
                handle_mcp_approval(&self.options, tool, mcp_tool, mcp_registry)
            }
            Some(ToolRiskLevel::Network) => {
                if self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_network)
                    == Some(true)
                {
                    if let Some(domain) = extract_domain(tool) {
                        let denied = self
                            .options
                            .network
                            .as_ref()
                            .and_then(|n| n.denied_domains.as_ref())
                            .map(|d| d.iter().any(|d| domain_matches(&domain, d)))
                            .unwrap_or(false);
                        if denied {
                            return ApprovalDecision::Deny(format!(
                                "Domain {} is in denylist",
                                domain
                            ));
                        }
                        let allowed = self
                            .options
                            .network
                            .as_ref()
                            .and_then(|n| n.allowed_domains.as_ref());
                        if let Some(allowed) = allowed {
                            if !allowed.is_empty()
                                && !allowed.iter().any(|d| domain_matches(&domain, d))
                            {
                                return ApprovalDecision::Ask;
                            }
                        }
                        ApprovalDecision::Approve
                    } else {
                        ApprovalDecision::Ask
                    }
                } else {
                    ApprovalDecision::Ask
                }
            }
            Some(ToolRiskLevel::System) => ApprovalDecision::Ask,
            Some(ToolRiskLevel::Interaction) => {
                let allow = self
                    .options
                    .categories
                    .as_ref()
                    .and_then(|c| c.always_allow_interaction)
                    .unwrap_or(true);
                if allow {
                    ApprovalDecision::Approve
                } else {
                    ApprovalDecision::Ask
                }
            }
            None => ApprovalDecision::Ask,
        }
    }

    pub fn record_response(
        &self,
        batch: &mut ToolBatch,
        tool_call_id: &str,
        response: ToolApprovalResponseData,
    ) {
        batch.results.insert(tool_call_id.to_string(), response);
    }

    pub fn is_batch_complete(&self, batch: &ToolBatch) -> bool {
        batch.results.len() == batch.pending.len()
    }

    pub fn get_approval_result(
        &self,
        batch: &ToolBatch,
        tool_call_id: &str,
    ) -> Option<ToolApprovalResult> {
        batch.results.get(tool_call_id).map(|r| ToolApprovalResult {
            approved: r.approved,
            tool_call_id: tool_call_id.to_string(),
            edited_parameters: r.edited_parameters.clone(),
            rejection_reason: r.rejection_reason.clone(),
        })
    }

    pub fn risk_level_from_str(level: &str) -> Option<ToolRiskLevel> {
        match level {
            "read_only" => Some(ToolRiskLevel::ReadOnly),
            "write" => Some(ToolRiskLevel::Write),
            "execute" => Some(ToolRiskLevel::Execute),
            "mcp" => Some(ToolRiskLevel::Mcp),
            "network" => Some(ToolRiskLevel::Network),
            "system" => Some(ToolRiskLevel::System),
            "interaction" => Some(ToolRiskLevel::Interaction),
            _ => None,
        }
    }
}

impl Default for ToolApprovalCoordinator {
    fn default() -> Self {
        Self::with_defaults()
    }
}

fn handle_execute_command_approval(
    options: &ToolApprovalOptions,
    tool: &ToolApprovalRequestData,
) -> ApprovalDecision {
    if options
        .categories
        .as_ref()
        .and_then(|c| c.always_allow_execute)
        != Some(true)
    {
        return ApprovalDecision::Ask;
    }

    let command = tool
        .parameters
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Reuse the engine-level CommandPolicy instead of re-implementing the
    // decision here: when the approval layer derives its command settings from
    // the same ShellToolConfig (see command_approval_settings_from_shell),
    // both layers always agree.
    let allowed = options
        .command
        .as_ref()
        .and_then(|c| c.allowed_commands.clone())
        .unwrap_or_default();
    let denied = options
        .command
        .as_ref()
        .and_then(|c| c.denied_commands.clone());
    let policy = CommandPolicy::new(allowed, denied);

    match policy.decision(command) {
        CommandDecision::AutoApprove => ApprovalDecision::Approve,
        CommandDecision::AutoDeny => {
            ApprovalDecision::Deny("Command is in denylist or not in allowlist".to_string())
        }
        CommandDecision::AskUser => ApprovalDecision::Ask,
    }
}

/// Derive the command approval settings for [`ToolApprovalOptions`] from a
/// [`ShellToolConfig`], so the approval layer and the engine baseline share
/// the same allow/deny lists (single config source, no double-source drift).
pub fn command_approval_settings_from_shell(config: &ShellToolConfig) -> CommandApprovalSettings {
    CommandApprovalSettings {
        allowed_commands: Some(config.allowed_commands.clone()),
        denied_commands: config.denied_commands.clone(),
    }
}
