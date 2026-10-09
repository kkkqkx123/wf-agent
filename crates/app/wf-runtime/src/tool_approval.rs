//! Shared tool-approval policy for hosts that decide without interaction.
//!
//! The engine gate (`ToolApprovalCoordinator`) runs first on every tool call;
//! only its `Ask` decisions reach a [`ToolApprovalHandler`]. Interactive
//! hosts answer those through their own UI (CLI prompt, TUI view, persisted
//! server interaction); non-interactive hosts answer them from this module,
//! either with the pure [`ApprovalPolicy`] (pre-authorized prefixes and
//! low-risk tools are allowed, everything else is denied) or with the
//! fail-closed [`LlmApprovalHandler`] reviewer.
//!
//! This lives in the runtime (not in any CLI crate) so the server and every
//! CLI form share one policy: the CLI name lists used to live in
//! `wf-cli-shared` where the server could not reuse them, and the LLM
//! reviewer used to live there too.

use std::sync::Arc;

use serde_json::Value;
use wf_api::{ToolApprovalHandler, ToolApprovalRequest, ToolApprovalResult};
use wf_llm::LlmGateway;
use wf_types::llm::LlmRequest;
use wf_types::message::Message;

/// Verdicts the LLM reviewer accepts; anything else fails closed.
const VERDICT_ALLOW: &str = "ALLOW";
const VERDICT_DENY: &str = "DENY";

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

/// Model-reviewed [`ToolApprovalHandler`]: answers the engine's `Ask`
/// decisions by asking an LLM whether the pending tool call is safe, for
/// hosts that run unattended yet still want a safety review instead of a
/// blanket deny.
///
/// The reviewer system prompt is a built-in resource, shared by every host
/// that reviews tool calls with a model. The handler holds only the gateway
/// and the reviewing profile id, so any host with a gateway can build it.
///
/// Fail-closed by construction: a gateway failure and an unparseable verdict
/// both reject the tool call, with the reason preserved for the transcript.
pub struct LlmApprovalHandler {
    gateway: Arc<LlmGateway>,
    profile_id: String,
}

impl LlmApprovalHandler {
    pub fn new(gateway: Arc<LlmGateway>, profile_id: impl Into<String>) -> Self {
        Self {
            gateway,
            profile_id: profile_id.into(),
        }
    }

    async fn decide(&self, request: &ToolApprovalRequest) -> Result<bool, String> {
        let llm_request = LlmRequest {
            profile_id: self.profile_id.clone(),
            messages: vec![
                Message::system_text(
                    wf_resource::embedded_assets::approval_reviewer_prompt().to_string(),
                ),
                Message::user_text(format!(
                    "Tool: {}\nArguments: {}\nIs it safe to execute? Reply ALLOW or DENY.",
                    request.tool_name, request.arguments
                )),
            ],
            parameters: None,
            generation: None,
            tools: None,
            tool_call_protocol: None,
            locked_tool_call_protocol: None,
            violation_policy: None,
            execution_id: None,
            stream: None,
            dead_loop_detection: None,
            protocol_auto_converted: None,
            timeout_ms: None,
        };
        let result = wf_execution_shared::generate_text_once(&self.gateway, &llm_request, None)
            .await
            .map_err(|e| e.to_string())?;
        let verdict = result
            .content
            .unwrap_or_default()
            .trim()
            .to_ascii_uppercase();
        if verdict.starts_with(VERDICT_ALLOW) {
            Ok(true)
        } else if verdict.starts_with(VERDICT_DENY) {
            Ok(false)
        } else {
            Err(format!("ambiguous verdict: {verdict}"))
        }
    }
}

#[async_trait::async_trait]
impl ToolApprovalHandler for LlmApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        match self.decide(request).await {
            Ok(true) => ToolApprovalResult::approved(request.tool_call_id.clone()),
            Ok(false) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                "denied by LLM safety review".to_string(),
            ),
            Err(reason) => ToolApprovalResult::rejected(
                request.tool_call_id.clone(),
                format!("LLM approval unavailable ({reason}); failing closed"),
            ),
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
    use wf_llm::mock::{LlmResponseSpec, MockLlmClient};

    fn mock_profile(id: &str) -> wf_types::llm::LlmProfile {
        wf_types::llm::LlmProfile {
            id: id.to_string(),
            name: id.to_string(),
            format: wf_types::llm::LlmFormat::OpenaiChat,
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
            proxy: None,
            circuit_breaker: None,
        }
    }

    /// Gateway serving `verdict` for profile `reviewer`.
    fn reviewer_gateway(verdict: &str) -> Arc<LlmGateway> {
        let gateway = Arc::new(LlmGateway::new());
        let mock = Arc::new(MockLlmClient::new());
        mock.default(LlmResponseSpec::text(verdict));
        gateway.register_mock("reviewer", mock);
        gateway
            .register_profile(mock_profile("reviewer"))
            .expect("test profile registers");
        gateway
    }

    fn request(tool: &str) -> ToolApprovalRequest {
        ToolApprovalRequest {
            tool_call_id: "call-1".into(),
            tool_name: tool.into(),
            arguments: serde_json::json!({ "command": "ls" }),
            interaction_id: "approval-1".into(),
            risk_level: None,
            tool_description: None,
            batch_id: None,
            tool_index: None,
            total_tools: None,
            pending_queue: None,
        }
    }

    #[tokio::test]
    async fn allow_verdict_approves_the_call() {
        let handler = LlmApprovalHandler::new(reviewer_gateway("ALLOW"), "reviewer");
        let result = handler.request_approval(&request("execute_command")).await;
        assert!(result.approved);
        assert_eq!(result.tool_call_id, "call-1");
    }

    #[tokio::test]
    async fn deny_verdict_rejects_the_call() {
        let handler = LlmApprovalHandler::new(reviewer_gateway("DENY"), "reviewer");
        let result = handler.request_approval(&request("execute_command")).await;
        assert!(!result.approved);
        assert_eq!(
            result.rejection_reason.as_deref(),
            Some("denied by LLM safety review")
        );
    }

    #[tokio::test]
    async fn verdict_matching_ignores_surrounding_text_and_case() {
        let handler = LlmApprovalHandler::new(reviewer_gateway("allow\n"), "reviewer");
        assert!(
            handler
                .request_approval(&request("execute_command"))
                .await
                .approved
        );
    }

    #[tokio::test]
    async fn ambiguous_verdict_fails_closed() {
        let handler = LlmApprovalHandler::new(reviewer_gateway("maybe"), "reviewer");
        let result = handler.request_approval(&request("execute_command")).await;
        assert!(!result.approved);
        assert!(result
            .rejection_reason
            .unwrap()
            .contains("ambiguous verdict"));
    }

    #[tokio::test]
    async fn gateway_failure_fails_closed() {
        let gateway = Arc::new(LlmGateway::new());
        // No profile registered: the gateway cannot resolve the reviewer.
        let handler = LlmApprovalHandler::new(gateway, "absent-profile");
        let result = handler.request_approval(&request("execute_command")).await;
        assert!(!result.approved);
        assert!(result.rejection_reason.unwrap().contains("failing closed"));
    }

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
