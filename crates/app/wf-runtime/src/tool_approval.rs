//! Shared tool-approval policy for hosts that decide without interaction.
//!
//! The engine gate (`ToolApprovalCoordinator`) runs first on every tool call;
//! only its `Ask` decisions reach a [`ToolApprovalHandler`]. Interactive
//! hosts answer those through their own UI (CLI prompt, TUI view, persisted
//! server interaction); non-interactive hosts answer them with the pure
//! [`ApprovalPolicy`] in this module: pre-authorized prefixes and low-risk
//! tools are allowed, everything else is denied.
//!
//! This lives in the runtime (not in any CLI crate) so the server and every
//! CLI form share one policy: the CLI name lists used to live in
//! `wf-cli-shared` where the server could not reuse them.

use std::sync::Arc;

use serde_json::Value;
use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};

/// Default tools that mutate state or execute commands: denied unless
/// covered by an explicit pre-authorization prefix.
pub fn default_sensitive_tools() -> Vec<&'static str> {
    vec![
        "approve_changes",
        "write_file",
        "edit_file",
        "apply_patch",
        "apply_diff",
        "execute_command",
    ]
}

/// Default read-only and side-effect-free tools allowed without interaction.
pub fn default_low_risk_tools() -> Vec<&'static str> {
    vec![
        "read_file",
        "list_files",
        "grep_search",
        "glob_search",
        "update_todo_list",
        "skill",
    ]
}

/// Argument keys inspected for command pre-authorization prefixes.
pub const COMMAND_ARGUMENT_KEYS: &[&str] = &["command", "cmd"];

/// Outcome of the headless approval decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Allow { reason: String },
    Deny { reason: String },
}

/// Pure headless approval policy: sensitive tools are denied, pre-authorized
/// prefixes allow execution, low-risk tools are allowed, everything else is
/// denied with a hint.
///
/// The `sensitive_tools` and `low_risk_tools` lists are configurable; when
/// `None` the built-in defaults are used. This allows runtime config to
/// override the headless approval lists without changing caller code.
#[derive(Debug, Clone)]
pub struct ApprovalPolicy {
    approve_prefixes: Vec<String>,
    sensitive_tools: Vec<String>,
    low_risk_tools: Vec<String>,
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            approve_prefixes: Vec::new(),
            sensitive_tools: default_sensitive_tools()
                .into_iter()
                .map(String::from)
                .collect(),
            low_risk_tools: default_low_risk_tools()
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

impl ApprovalPolicy {
    pub fn new(approve_prefixes: Vec<String>) -> Self {
        Self {
            approve_prefixes,
            ..Default::default()
        }
    }

    /// Builder: override the sensitive tools list.
    pub fn with_sensitive_tools(mut self, tools: Vec<String>) -> Self {
        self.sensitive_tools = tools;
        self
    }

    /// Builder: override the low-risk tools list.
    pub fn with_low_risk_tools(mut self, tools: Vec<String>) -> Self {
        self.low_risk_tools = tools;
        self
    }

    pub fn decide(&self, tool_name: &str, arguments: &Value) -> ApprovalDecision {
        if self.prefix_matches(tool_name, arguments) {
            return ApprovalDecision::Allow {
                reason: "pre-authorized by --approve-prefix".to_string(),
            };
        }
        if self.sensitive_tools.iter().any(|t| t == tool_name) {
            return ApprovalDecision::Deny {
                reason: format!(
                    "sensitive tool '{tool_name}' requires interactive approval; \
                     denied in headless mode"
                ),
            };
        }
        if self.low_risk_tools.iter().any(|t| t == tool_name) {
            return ApprovalDecision::Allow {
                reason: "low-risk tool allow-listed for headless runs".to_string(),
            };
        }
        ApprovalDecision::Deny {
            reason: format!(
                "tool '{tool_name}' is not on the headless allow-list; \
                 pass --approve-prefix '{tool_name}' to pre-authorize it"
            ),
        }
    }

    fn prefix_matches(&self, tool_name: &str, arguments: &Value) -> bool {
        let mut candidates: Vec<&str> = vec![tool_name];
        for key in COMMAND_ARGUMENT_KEYS {
            if let Some(command) = arguments.get(*key).and_then(Value::as_str) {
                candidates.push(command);
            }
        }
        self.approve_prefixes
            .iter()
            .any(|prefix| candidates.iter().any(|c| c.starts_with(prefix.as_str())))
    }
}

/// One policy decision handed to a [`DecisionReporter`].
#[derive(Debug, Clone)]
pub struct PolicyReport {
    pub tool_call_id: String,
    pub tool_name: String,
    pub allowed: bool,
    pub reason: String,
}

/// Optional sink for policy decisions. Hosts without one (server-side) get
/// `tracing` diagnostics; the CLI headless runner mirrors them into its
/// diagnostics channel to preserve its `ok/err` output contract.
pub type DecisionReporter = Arc<dyn Fn(PolicyReport) + Send + Sync>;

/// Policy-only [`ToolApprovalHandler`]: answers the engine's `Ask`
/// decisions from [`ApprovalPolicy`] without contacting a human. Hosts that
/// can ask a human register their own handler instead; this one is fail-
/// closed by construction (unknown tools deny).
pub struct PolicyApprovalHandler {
    policy: ApprovalPolicy,
    reporter: Option<DecisionReporter>,
}

impl PolicyApprovalHandler {
    pub fn new(policy: ApprovalPolicy) -> Self {
        Self {
            policy,
            reporter: None,
        }
    }

    pub fn with_reporter(mut self, reporter: DecisionReporter) -> Self {
        self.reporter = Some(reporter);
        self
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for PolicyApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        let (allowed, reason) = match self.policy.decide(&request.tool_name, &request.arguments) {
            ApprovalDecision::Allow { reason } => (true, reason),
            ApprovalDecision::Deny { reason } => (false, reason),
        };
        let report = PolicyReport {
            tool_call_id: request.tool_call_id.clone(),
            tool_name: request.tool_name.clone(),
            allowed,
            reason: reason.clone(),
        };
        match &self.reporter {
            Some(reporter) => reporter(report),
            None => {
                if allowed {
                    tracing::info!(tool = %request.tool_name, reason = %reason, "policy allowed tool");
                } else {
                    tracing::warn!(tool = %request.tool_name, reason = %reason, "policy denied tool");
                }
            }
        }
        if allowed {
            ToolApprovalResult::approved(request.tool_call_id.clone())
        } else {
            ToolApprovalResult::rejected(request.tool_call_id.clone(), reason)
        }
    }
}

/// Effective engine policy for hosts that always attach a handler (CLI
/// forms, server headless runs): the host config's overrides resolved over
/// the balanced baseline when enabled, otherwise the baseline itself. The
/// engine evaluates this first, so policy denials stay terminal even though
/// a handler is attached.
///
/// This is deliberately different from the opt-in host default (no wiring
/// at all when disabled): callers of this helper already decided a handler
/// is attached, so "no policy" must not degrade into ask-everything.
pub fn headless_approval_options(
    ctx: Option<&wf_api::infra::context::ApiContext>,
) -> wf_types::tool::approval::ToolApprovalOptions {
    match ctx.and_then(|ctx| ctx.tool_approval.as_ref()) {
        Some(config) if config.enabled => config.resolved_options(),
        _ => wf_types::tool::approval::ToolApprovalOptions::balanced_defaults(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_tools_are_denied_with_reason() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in default_sensitive_tools() {
            match policy.decide(tool, &serde_json::json!({})) {
                ApprovalDecision::Deny { reason } => {
                    assert!(reason.contains("sensitive"), "{tool}: {reason}");
                    assert!(reason.contains(tool), "{tool}: {reason}");
                }
                other => panic!("{tool} should be denied, got {other:?}"),
            }
        }
    }

    #[test]
    fn low_risk_tools_are_allowed() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in default_low_risk_tools() {
            assert!(
                matches!(
                    policy.decide(tool, &serde_json::json!({})),
                    ApprovalDecision::Allow { .. }
                ),
                "{tool} should be allowed"
            );
        }
    }

    #[test]
    fn unknown_tools_are_denied_with_hint() {
        let policy = ApprovalPolicy::new(vec![]);
        match policy.decide("rm_rf_everything", &serde_json::json!({})) {
            ApprovalDecision::Deny { reason } => {
                assert!(reason.contains("--approve-prefix"), "{reason}")
            }
            other => panic!("unknown tool should be denied, got {other:?}"),
        }
    }

    #[test]
    fn prefix_preauthorizes_tool_names_and_commands() {
        let policy = ApprovalPolicy::new(vec!["git".to_string()]);
        assert!(matches!(
            policy.decide("git_status_custom", &serde_json::json!({})),
            ApprovalDecision::Allow { .. }
        ));
        // The prefix explicitly consents to sensitive tools too: a `git`
        // prefix authorizes `execute_command` running `git status`.
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Allow { .. }
        ));
        // Prefixes are literal: "git" does not authorize unrelated commands.
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "rm -rf /" })
            ),
            ApprovalDecision::Deny { .. }
        ));
        // Without any prefix the sensitive tool stays denied.
        let strict = ApprovalPolicy::new(vec![]);
        assert!(matches!(
            strict.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Deny { .. }
        ));
    }

    #[test]
    fn custom_lists_override_defaults() {
        let policy = ApprovalPolicy::new(vec![])
            .with_sensitive_tools(vec!["custom_write".to_string()])
            .with_low_risk_tools(vec!["custom_read".to_string()]);
        assert!(matches!(
            policy.decide("custom_write", &serde_json::json!({})),
            ApprovalDecision::Deny { .. }
        ));
        assert!(matches!(
            policy.decide("custom_read", &serde_json::json!({})),
            ApprovalDecision::Allow { .. }
        ));
        // Default sensitive tools no longer denied when overridden.
        assert!(matches!(
            policy.decide("write_file", &serde_json::json!({})),
            ApprovalDecision::Deny { .. }
        ));
    }
}
