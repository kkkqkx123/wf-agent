use serde_json::Value;

/// Default tools that mutate state or execute commands: always denied in
/// headless runs unless covered by an explicit `--approve-prefix`.
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

/// Default read-only and side-effect-free tools auto-approved in headless
/// runs.
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
/// override the headless approval lists without changing CLI code.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_tools_are_denied() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in default_sensitive_tools() {
            match policy.decide(tool, &serde_json::json!({})) {
                ApprovalDecision::Deny { reason } => {
                    assert!(reason.contains("sensitive"));
                }
                other => panic!("{tool} should be denied, got {other:?}"),
            }
        }
    }

    #[test]
    fn low_risk_tools_are_allowed() {
        let policy = ApprovalPolicy::new(vec![]);
        for tool in default_low_risk_tools() {
            assert!(matches!(
                policy.decide(tool, &serde_json::json!({})),
                ApprovalDecision::Allow { .. }
            ));
        }
    }

    #[test]
    fn prefix_preauthorizes() {
        let policy = ApprovalPolicy::new(vec!["git".to_string()]);
        assert!(matches!(
            policy.decide("git_status_custom", &serde_json::json!({})),
            ApprovalDecision::Allow { .. }
        ));
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "git status" })
            ),
            ApprovalDecision::Allow { .. }
        ));
        assert!(matches!(
            policy.decide(
                "execute_command",
                &serde_json::json!({ "command": "rm -rf /" })
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
